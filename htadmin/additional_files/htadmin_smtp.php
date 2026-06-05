<?php
include_once ('includes/checklogin.php');
include_once ('includes/head.php');
include_once ('includes/nav.php');

$db_path = '/mnt/clapshot-data/data/clapshot.sqlite';
$message = '';
$message_type = '';

// Ouvrir la base SQLite
try {
    $db = new SQLite3($db_path, SQLITE3_OPEN_READWRITE);
} catch (Exception $e) {
    die('<div class="container"><div class="alert alert-danger">Impossible d\'ouvrir la base de données : ' . htmlspecialchars($e->getMessage()) . '</div></div>');
}

// Lire la config actuelle
function get_setting($db, $key, $default = '') {
    $stmt = $db->prepare('SELECT value FROM settings WHERE key = :key');
    $stmt->bindValue(':key', $key, SQLITE3_TEXT);
    $result = $stmt->execute();
    $row = $result->fetchArray(SQLITE3_ASSOC);
    return $row ? $row['value'] : $default;
}

function set_setting($db, $key, $value) {
    $stmt = $db->prepare('INSERT OR REPLACE INTO settings (key, value) VALUES (:key, :value)');
    $stmt->bindValue(':key', $key, SQLITE3_TEXT);
    $stmt->bindValue(':value', $value, SQLITE3_TEXT);
    $stmt->execute();
}

// Traitement du formulaire
if ($_SERVER['REQUEST_METHOD'] === 'POST' && isset($_POST['smtp_host'])) {
    $host     = trim($_POST['smtp_host'] ?? '');
    $port     = trim($_POST['smtp_port'] ?? '587');
    $user     = trim($_POST['smtp_user'] ?? '');
    $password = $_POST['smtp_password'] ?? '';
    $from     = trim($_POST['smtp_from'] ?? '');

    set_setting($db, 'smtp_host', $host);
    set_setting($db, 'smtp_port', $port ?: '587');
    set_setting($db, 'smtp_user', $user);
    set_setting($db, 'smtp_from', $from);

    // Ne mettre à jour le mot de passe que s'il est fourni
    if (!empty($password)) {
        set_setting($db, 'smtp_password', $password);
    }

    if (!empty($host)) {
        $message = 'Configuration SMTP enregistrée avec succès.';
        $message_type = 'success';
    } else {
        $message = 'Configuration SMTP effacée — les emails ne seront plus envoyés.';
        $message_type = 'info';
    }
}

if (isset($_POST['clear_smtp'])) {
    foreach (['smtp_host', 'smtp_port', 'smtp_user', 'smtp_password', 'smtp_from'] as $k) {
        set_setting($db, $k, '');
    }
    $message = 'Configuration SMTP effacée.';
    $message_type = 'info';
}

// Lire les valeurs actuelles
$smtp_host     = get_setting($db, 'smtp_host');
$smtp_port     = get_setting($db, 'smtp_port', '587');
$smtp_user     = get_setting($db, 'smtp_user');
$smtp_password = get_setting($db, 'smtp_password');
$smtp_from     = get_setting($db, 'smtp_from');
$is_configured = !empty($smtp_host);
?>

<div class="container box">
    <div class="row">
        <div class="col-xs-12">
            <h2>✉️ Configuration Mail (SMTP)</h2>
            <p class="text-muted">
                Paramètres utilisés pour l'envoi des emails de notification (réponses aux commentaires).
                Ces paramètres sont appliqués immédiatement, sans redémarrage du serveur.
            </p>

            <?php if ($message): ?>
            <div class="alert alert-<?php echo $message_type; ?>">
                <?php echo htmlspecialchars($message); ?>
            </div>
            <?php endif; ?>

            <div class="panel panel-default">
                <div class="panel-heading" style="background-color:#917a49;color:white;">
                    <h3 class="panel-title">
                        Statut :
                        <?php if ($is_configured): ?>
                            <span class="label label-success">✓ Configuré — <?php echo htmlspecialchars($smtp_host); ?>:<?php echo htmlspecialchars($smtp_port); ?></span>
                        <?php else: ?>
                            <span class="label label-danger">✗ Non configuré</span>
                        <?php endif; ?>
                    </h3>
                </div>
                <div class="panel-body">
                    <form method="post" action="smtp.php">
                        <div class="form-group">
                            <label>Serveur SMTP <small class="text-muted">(ex: mail.example.com)</small></label>
                            <input type="text" class="form-control" name="smtp_host"
                                   value="<?php echo htmlspecialchars($smtp_host); ?>"
                                   placeholder="mail.example.com">
                        </div>
                        <div class="form-group">
                            <label>Port SMTP</label>
                            <select class="form-control" name="smtp_port">
                                <option value="587" <?php echo $smtp_port == '587' ? 'selected' : ''; ?>>587 — STARTTLS (recommandé)</option>
                                <option value="465" <?php echo $smtp_port == '465' ? 'selected' : ''; ?>>465 — SSL/TLS</option>
                                <option value="1025" <?php echo $smtp_port == '1025' ? 'selected' : ''; ?>>1025 — Local / MailHog (test)</option>
                                <option value="25"   <?php echo $smtp_port == '25'   ? 'selected' : ''; ?>>25 — SMTP classique</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>Utilisateur SMTP</label>
                            <input type="text" class="form-control" name="smtp_user"
                                   value="<?php echo htmlspecialchars($smtp_user); ?>"
                                   placeholder="contact@example.com" autocomplete="username">
                        </div>
                        <div class="form-group">
                            <label>Mot de passe SMTP
                                <?php if (!empty($smtp_password)): ?>
                                    <small class="text-success">(défini — laisser vide pour ne pas changer)</small>
                                <?php endif; ?>
                            </label>
                            <input type="password" class="form-control" name="smtp_password"
                                   placeholder="<?php echo !empty($smtp_password) ? '••••••••' : 'Mot de passe'; ?>"
                                   autocomplete="new-password">
                        </div>
                        <div class="form-group">
                            <label>Adresse expéditeur <small class="text-muted">(champ From)</small></label>
                            <input type="email" class="form-control" name="smtp_from"
                                   value="<?php echo htmlspecialchars($smtp_from); ?>"
                                   placeholder="noreply@example.com">
                        </div>
                        <button type="submit" class="btn btn-primary" style="background-color:#917a49;border-color:#7a6840;">
                            💾 Enregistrer
                        </button>
                    </form>

                    <?php if ($is_configured): ?>
                    <hr>
                    <form method="post" action="smtp.php"
                          onsubmit="return confirm('Effacer toute la configuration SMTP ?');">
                        <button type="submit" name="clear_smtp" class="btn btn-danger btn-sm">
                            🗑️ Effacer la configuration
                        </button>
                    </form>
                    <?php endif; ?>
                </div>
            </div>

            <div class="panel panel-info">
                <div class="panel-heading"><h4 class="panel-title">ℹ️ Comment ça marche ?</h4></div>
                <div class="panel-body">
                    <ul>
                        <li>Quand un utilisateur répond à un commentaire, l'auteur du commentaire reçoit un email.</li>
                        <li>L'adresse email de chaque utilisateur se configure dans l'<a href="/">interface Clapshot</a> (admin → clic-droit sur l'utilisateur → "Définir l'email").</li>
                        <li>Les modifications SMTP sont actives immédiatement, sans redémarrage.</li>
                    </ul>
                </div>
            </div>
        </div>
    </div>
</div>

<?php
include_once ('includes/footer.php');
$db->close();
?>
