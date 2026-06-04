use anyhow::{Context, Result};
use crate::database::{models, PooledConnection};
use diesel::prelude::*;
use lettre::{
    message::{header::ContentType, MultiPart, SinglePart},
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{Tls, TlsParameters},
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

/// SMTP configuration read from environment variables:
///   SMTP_HOST, SMTP_PORT (default 465), SMTP_USER, SMTP_PASSWORD, SMTP_FROM
#[derive(Clone, Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
}

impl SmtpConfig {
    /// Load from environment variables. Returns None if SMTP_HOST is not set.
    pub fn from_env() -> Option<SmtpConfig> {
        let host = std::env::var("SMTP_HOST").ok()?;
        let port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(465u16);
        let user = std::env::var("SMTP_USER").unwrap_or_default();
        let password = std::env::var("SMTP_PASSWORD").unwrap_or_default();
        let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| format!("clapshot@{}", host));
        Some(SmtpConfig { host, port, user, password, from })
    }

    /// Load from the `settings` table in the DB.
    /// DB settings take priority over environment variables.
    /// Returns None if no SMTP host is configured in DB or env.
    pub fn from_db_or_env(conn: &mut PooledConnection) -> Option<SmtpConfig> {
        use crate::database::schema::settings::dsl::*;

        let rows: Vec<models::Setting> = settings
            .load::<models::Setting>(conn)
            .unwrap_or_default();

        let get = |k: &str| -> Option<String> {
            rows.iter().find(|r| r.key == k).map(|r| r.value.clone())
        };

        let host = get("smtp_host").or_else(|| std::env::var("SMTP_HOST").ok())?;
        if host.is_empty() { return None; }

        let port = get("smtp_port")
            .and_then(|p| p.parse().ok())
            .or_else(|| std::env::var("SMTP_PORT").ok().and_then(|p| p.parse().ok()))
            .unwrap_or(465u16);
        let user = get("smtp_user").or_else(|| std::env::var("SMTP_USER").ok()).unwrap_or_default();
        let password = get("smtp_password").or_else(|| std::env::var("SMTP_PASSWORD").ok()).unwrap_or_default();
        let from = get("smtp_from")
            .or_else(|| std::env::var("SMTP_FROM").ok())
            .unwrap_or_else(|| format!("clapshot@{}", host));

        Some(SmtpConfig { host, port, user, password, from })
    }

    /// Save SMTP config to the `settings` table in the DB.
    pub fn save_to_db(&self, conn: &mut PooledConnection) -> anyhow::Result<()> {
        use crate::database::schema::settings::dsl::*;
        use diesel::replace_into;

        let pairs = vec![
            ("smtp_host",     self.host.clone()),
            ("smtp_port",     self.port.to_string()),
            ("smtp_user",     self.user.clone()),
            ("smtp_password", self.password.clone()),
            ("smtp_from",     self.from.clone()),
        ];
        for (k, v) in pairs {
            replace_into(settings)
                .values(&models::SettingInsert { key: k.to_string(), value: v })
                .execute(conn)
                .context("Failed to save SMTP setting")?;
        }
        Ok(())
    }
}

/// Construire le transport SMTP (rustls, sans vérification stricte du certificat).
fn build_mailer(cfg: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let creds = Credentials::new(cfg.user.clone(), cfg.password.clone());

    let tls = TlsParameters::builder(cfg.host.clone())
        .dangerous_accept_invalid_certs(true)
        .build_rustls()
        .context("Failed to build TLS parameters")?;

    let tls_mode = if cfg.port == 465 {
        Tls::Wrapper(tls)
    } else if cfg.port == 587 {
        Tls::Required(tls)
    } else {
        Tls::None
    };

    Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host)
        .port(cfg.port)
        .tls(tls_mode)
        .credentials(creds)
        .authentication(vec![Mechanism::Login, Mechanism::Plain])
        .build())
}

/// Envoyer la notification de réponse à un commentaire (HTML + texte brut).
pub async fn send_reply_notification(
    cfg: &SmtpConfig,
    to: &str,
    subject: &str,
    replier_name: &str,
    reply_text: &str,
    video_title: &str,
    video_url: &str,
) -> Result<()> {
    let text = format!(
        "Bonjour,\n\n{replier_name} a répondu à votre commentaire sur la vidéo « {video_title} » :\n\n« {reply_text} »\n\nVoir la vidéo : {video_url}\n\n—\nL'équipe Powerloop"
    );

    let html = format!(r#"<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="margin:0;padding:0;background:#f4f4f4;font-family:Arial,sans-serif;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f4;padding:30px 0;">
    <tr><td align="center">
      <table width="600" cellpadding="0" cellspacing="0" style="background:#ffffff;border-radius:8px;overflow:hidden;box-shadow:0 2px 8px rgba(0,0,0,0.1);">

        <!-- Header -->
        <tr>
          <td style="background:#917a49;padding:28px 32px;">
            <h1 style="margin:0;color:#ffffff;font-size:22px;font-weight:bold;letter-spacing:1px;">
              🎬 POWERLOOP MEDIA
            </h1>
          </td>
        </tr>

        <!-- Body -->
        <tr>
          <td style="padding:32px;">
            <p style="margin:0 0 16px;color:#333333;font-size:16px;">Bonjour,</p>
            <p style="margin:0 0 24px;color:#333333;font-size:16px;">
              <strong>{replier_name}</strong> a répondu à votre commentaire sur la vidéo :
            </p>

            <!-- Video title -->
            <div style="background:#faf7f0;border-left:4px solid #917a49;border-radius:4px;padding:12px 16px;margin-bottom:24px;">
              <p style="margin:0;color:#917a49;font-size:13px;font-weight:bold;text-transform:uppercase;letter-spacing:0.5px;">Vidéo</p>
              <p style="margin:4px 0 0;color:#1a1a2e;font-size:15px;font-weight:bold;">🎥 {video_title}</p>
            </div>

            <!-- Reply text -->
            <div style="background:#f9f9f9;border-left:4px solid #e0e0e0;border-radius:4px;padding:16px;margin-bottom:28px;">
              <p style="margin:0 0 8px;color:#999999;font-size:12px;font-weight:bold;text-transform:uppercase;letter-spacing:0.5px;">Réponse de {replier_name}</p>
              <p style="margin:0;color:#333333;font-size:15px;line-height:1.6;font-style:italic;">« {reply_text} »</p>
            </div>

            <!-- CTA -->
            <table cellpadding="0" cellspacing="0">
              <tr>
                <td style="border-radius:6px;background:#917a49;">
                  <a href="{video_url}" style="display:inline-block;padding:14px 28px;color:#ffffff;font-size:15px;font-weight:bold;text-decoration:none;border-radius:6px;">
                    Voir la vidéo →
                  </a>
                </td>
              </tr>
            </table>
          </td>
        </tr>

        <!-- Footer -->
        <tr>
          <td style="background:#f4f4f4;padding:20px 32px;border-top:1px solid #e0e0e0;">
            <p style="margin:0;color:#999999;font-size:12px;text-align:center;">
              Cet email a été envoyé automatiquement par Powerloop Media · <a href="{video_url}" style="color:#917a49;text-decoration:none;">Accéder à la plateforme</a>
            </p>
          </td>
        </tr>

      </table>
    </td></tr>
  </table>
</body>
</html>"#);

    let email = Message::builder()
        .from(cfg.from.parse().context("Invalid SMTP_FROM address")?)
        .to(to.parse().context("Invalid recipient email address")?)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(text))
                .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(html)),
        )
        .context("Failed to build email message")?;

    let mailer = build_mailer(cfg)?;
    mailer.send(email).await.context("Failed to send email")?;
    Ok(())
}
