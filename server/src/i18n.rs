//! Server-side localization of user-facing messages.
//!
//! gettext-style: the message id IS the English source string. Translations come from the central
//! PO catalogs (i18n/po/<locale>.po), compiled into a static lookup at build time (see build.rs),
//! so there are no runtime files. A missing/untranslated entry falls back to the source string.
//!
//! Messages are localized to the recipient user's UI locale (forwarded by the client as
//! UserSessionData.language and looked up via `ServerState::locale_for_user`). For `format!`-style
//! messages, mark the source with named `{placeholders}` and interpolate after translating, e.g.
//! `tr_fmt(loc, "{kind} deleted.", &[("kind", &kind)])`.

include!(concat!(env!("OUT_DIR"), "/i18n_catalog.rs"));  // pub fn translate(locale, msgid) -> Option<&'static str>

/// Translate `msgid` (the English source) to `locale`, falling back to the source string.
pub fn tr(locale: Option<&str>, msgid: &str) -> String {
    locale.and_then(|l| translate(l, msgid)).unwrap_or(msgid).to_string()
}

/// Translate `msgid` then substitute named `{placeholder}`s. Unknown placeholders are left intact.
pub fn tr_fmt(locale: Option<&str>, msgid: &str, params: &[(&str, &str)]) -> String {
    interpolate(&tr(locale, msgid), params)
}

/// Replace `{name}` occurrences with their values (the translation-runtime equivalent of `format!`).
pub fn interpolate(template: &str, params: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_source_when_untranslated() {
        assert_eq!(tr(Some("xx"), "Rename"), "Rename");
        assert_eq!(tr(None, "Rename"), "Rename");
    }

    #[test]
    fn interpolates_named_placeholders() {
        assert_eq!(tr_fmt(None, "{kind} deleted.", &[("kind", "Video")]), "Video deleted.");
    }
}
