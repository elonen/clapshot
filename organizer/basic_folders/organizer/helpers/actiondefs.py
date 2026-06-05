from textwrap import dedent
import clapshot_grpc.proto.clapshot as clap

class ActiondefsHelper:
    def __init__(self):
        pass

    def make_custom_actions_map(self) -> dict[str, clap.ActionDef]:
        """
        Popup actions for when the user right-clicks on a listing background.
        """
        return {
            "new_folder": self.make_new_folder_action(),
            "move_to_parent": self.make_move_to_parent_action(),
            "on_media_file_added": self.make_on_media_file_added_action(),
            "share_folder": self.make_share_folder_action(),
            "copy_shared_link": self.make_copy_shared_link_action(),
            "revoke_share": self.make_revoke_share_action(),
            "cleanup_empty_user": self.make_cleanup_empty_user_action(),
            "set_user_email": self.make_set_user_email_action(),
            "add_version": self.make_add_version_action(),
            "copy_media_id": self.make_copy_media_id_action(),
        }

    def make_new_folder_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Nouveau dossier",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-folder-plus", color=None)),
                key_shortcut=None,
                natural_desc="Créer un nouveau dossier"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder_name = (prompt("Nom du nouveau dossier", ""))?.trim();
                    if (folder_name) { clapshot.callOrganizer("new_folder", {name: folder_name}); }
                """).strip()))

    def make_move_to_parent_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Déplacer vers le parent",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-arrow-turn-up", color=None)),
                key_shortcut=None,
                natural_desc="Déplacer l'élément vers le dossier parent"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var listingData = _action_args.listing_data;
                    var items = _action_args.selected_items;

                    if (!listingData.parent_folder_id) {
                        alert("parent_folder_id manquant dans listingData.");
                        return;
                    }
                    var folderId = listingData.parent_folder_id;
                    var ids = clapshot.itemsToIDs(items);
                    clapshot.moveToFolder(folderId, ids, listingData);
                """).strip()))

    def make_on_media_file_added_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=None,  # not an UI action, just a callback
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var mfid = _action_args.media_file_id;
                    var listingData = _action_args.listing_data;
                    var folderId = listingData?.folder_id;

                    if (!folderId || !mfid) {
                        var msg = "Erreur on_media_file_added : media_file_id manquant, ou folder_id absent de listingData.";
                        alert(msg); console.error(msg);
                    } else {
                        clapshot.moveToFolder(folderId, [{mediaFileId: mfid}], listingData);
                    }
                """).strip()))

    def make_share_folder_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Partager le dossier",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-share-nodes", color=None)),
                key_shortcut=None,
                natural_desc="Créer un lien de partage pour ce dossier"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("Aucun dossier sélectionné à partager");
                        return;
                    }
                    clapshot.callOrganizer("share_folder", {id: folderId});
                """).strip()))

    def make_revoke_share_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Arrêter le partage",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-link-slash", color=None)),
                key_shortcut=None,
                natural_desc="Révoquer le lien de partage de ce dossier"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("Aucun dossier sélectionné à ne plus partager");
                        return;
                    }
                    if (confirm("Êtes-vous sûr de vouloir révoquer le lien de partage de ce dossier ?")) {
                        clapshot.callOrganizer("revoke_share", {id: folderId});
                    }
                """).strip()))

    def make_copy_shared_link_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Copier l'URL",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-copy", color=None)),
                key_shortcut=None,
                natural_desc="Copier le lien de partage dans le presse-papiers"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    var sharedFolderTokens = JSON.parse(_action_args.listing_data?.shared_folder_tokens || '{}');
                    var shareToken = sharedFolderTokens[folderId];

                    if (!shareToken) {
                        alert("Aucun lien de partage disponible pour ce dossier");
                        return;
                    }

                    // Construct the share URL using the current page's origin
                    var shareUrl = window.location.origin + "/?p=shared." + shareToken;

                    if (navigator.clipboard && navigator.clipboard.writeText) {
                        navigator.clipboard.writeText(shareUrl).then(function() {
                            alert('Lien de partage copié dans le presse-papiers !\\n\\nATTENTION : Partager un dossier donne accès permanent aux fichiers qu\\'il contient, même si vous révoquez le partage plus tard.');
                        }).catch(function() {
                            prompt('Copiez ce lien de partage :', shareUrl);
                        });
                    } else {
                        prompt('Copiez ce lien de partage :', shareUrl);
                    }
                """).strip()))

    def make_set_user_email_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Définir l'email",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-envelope", color=None)),
                key_shortcut=None,
                natural_desc="Définir l'adresse email pour les notifications de réponse"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderTitle = folder?.title || "";
                    var userId = folderTitle.replace(/ \\(.*\\)$/, "");
                    var currentEmail = (folderTitle.match(/\\((.+)\\)$/) || [])[1] || "";
                    if (currentEmail === "sans email") currentEmail = "";
                    var newEmail = prompt("Adresse email pour les notifications (" + userId + ") :", currentEmail);
                    if (newEmail !== null) {
                        clapshot.callOrganizer("set_user_email", {user_id: userId, email: newEmail.trim()});
                    }
                """).strip()))

    def make_copy_media_id_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Copier l'ID",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-fingerprint", color=None)),
                key_shortcut=None,
                natural_desc="Copier l'identifiant de la vidéo dans le presse-papiers"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var media = _action_args.selected_items?.[0]?.mediaFile;
                    var id = media?.id || null;
                    if (!id) { alert("Aucune vidéo sélectionnée"); return; }
                    if (navigator.clipboard && navigator.clipboard.writeText) {
                        navigator.clipboard.writeText(id).then(function() {
                            alert('ID copié : ' + id);
                        }).catch(function() { prompt('ID de la vidéo :', id); });
                    } else {
                        prompt('ID de la vidéo :', id);
                    }
                """).strip()))

    def make_add_version_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Ajouter une version",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-code-branch", color=None)),
                key_shortcut=None,
                natural_desc="Lier un autre fichier comme nouvelle version de cette vidéo"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var primary = _action_args.selected_items?.[0]?.mediaFile;
                    var primaryId = primary?.id || null;
                    if (!primaryId) { alert("Aucune vidéo sélectionnée"); return; }
                    var versionId = prompt("ID de la vidéo à lier comme nouvelle version :\\n(copiez l'ID depuis l'autre vidéo via son menu contextuel)");
                    if (versionId) {
                        clapshot.callOrganizer("link_as_version", {primary_id: primaryId, version_id: versionId.trim()});
                    }
                """).strip()))

    def make_cleanup_empty_user_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Suppr. utilisateur",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-user-minus", color=clap.Color(r=220, g=38, b=38))),
                key_shortcut=None,
                natural_desc="Supprimer l'utilisateur s'il n'a aucun contenu (uniquement un dossier vide)"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("Aucun dossier utilisateur sélectionné");
                        return;
                    }
                    if (confirm("Ceci supprimera l'utilisateur s'il n'a aucun fichier média et uniquement un dossier personnel vide.\\n\\nLes commentaires de cet utilisateur seront conservés mais marqués comme provenant d'un utilisateur supprimé.\\n\\nÊtes-vous sûr ?")) {
                        clapshot.callOrganizer("cleanup_empty_user", {folder_id: folderId});
                    }
                """).strip()))
