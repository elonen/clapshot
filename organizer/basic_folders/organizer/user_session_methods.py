from __future__ import annotations

import json
from typing import Optional
from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from clapshot_grpc.utilities import try_send_user_message, parse_json_dict

from organizer.config import PATH_COOKIE_NAME
from organizer.helpers.folders import SHARED_FOLDER_TOKEN_COOKIE_NAME
from organizer.utils import uri_arg_to_folder_path

from .database.models import DbFolder

import organizer


async def on_start_user_session_impl(oi: organizer.OrganizerInbound, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server when a user session is started, to define custom actions for the client.
    """
    assert req.ses.sid, "No session ID"

    # Get base actions from organizer
    actions = oi.actions_helper.make_custom_actions_map()

    # Let metaplugins extend/modify actions
    actions = oi.metaplugin_loader.call_extend_actions_hooks(actions)

    await oi.srv.client_define_actions(org.ClientDefineActionsRequest(
        sid = req.ses.sid,
        actions = actions))

    return org.OnStartUserSessionResponse()

async def navigate_page_impl(oi: organizer.OrganizerInbound, req: org.NavigatePageRequest) -> org.ClientShowPageRequest:
    """
    Organizer method (gRPC/protobuf)

    Server calls this to request Organizer to construct a navigation page for the Client to show.

    This is a "folder view" page (not a media player). Without an Organizer, the Server would just show
    a list of all media for the user. An Organizer can define a custom view,
    e.g. a folder tree or a list of categories, projects, even buttons etc.
    """
    ses = req.ses

    # If page ID starts with "shared.", e.g. ?p=shared.ABCD1234, it means the user has opened
    # a shared folder link.
    if req.page_id and req.page_id.startswith("shared."):
        share_token = req.page_id.split(".", 1)[1]
        with oi.db_new_session() as dbs:
            if share := await oi.folders_helper.get_share_by_token(dbs, share_token):
                req.page_id = str(share.folder_id)  # Replace token with folder ID for further processing
                ses.cookies[PATH_COOKIE_NAME] = json.dumps([share.folder_id])   # Use it as a new folder path root
                await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))

                # If current user is not the owner of the shared folder, store token in a cookie
                owner = await oi.folders_helper.get_folder_owner(dbs, share.folder_id)
                if owner and owner.id != ses.user.id:
                    ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token
                    await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))
            else:
                # Token not found? Reset session.
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=ses.sid,
                        msg=clap.UserMessage(
                            message="Ce lien de dossier partagé est invalide ou a été révoqué",
                            type=clap.UserMessageType.ERROR)))
                ses.cookies.pop(SHARED_FOLDER_TOKEN_COOKIE_NAME, None)
                ses.cookies.pop(PATH_COOKIE_NAME, None)
                await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))
                return await oi.pages_helper.construct_navi_page(ses, None)

    # Normal folder navigation, e.g. ?p=1.2.3
    cookie_override: Optional[str] = None
    if req.page_id:
        try:
            cookie_override = json.dumps(uri_arg_to_folder_path(req.page_id))
        except ValueError:
            oi.log.warning(f"Invalid folder path URI from client: '{req.page_id}'")
    else:
        # When OrganizerInbound.navigate_page() is called without a page_id, it means the user has opened the main page
        # without an URL parameter => we need to clear the folder_path cookie so other handlers don't push the wrong view.
        ses.cookies.pop(PATH_COOKIE_NAME, None)
        await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))

    return await oi.pages_helper.construct_navi_page(ses, cookie_override)


async def cmd_from_client_impl(oi: organizer.OrganizerInbound, cmd: org.CmdFromClientRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    These are usually triggered by user actions in the UI, and defined by the Organizer
    when a user session is started.

    The client doesn't really know what these commands do, it just executes action scripts
    that the organizer plugin has defined, e.g. for popup menus. The scripts can be anything,
    but they usually call these methods with the appropriate arguments.

    => These command names are organizer-specific and could be named anything.
    """
    try:
        args = parse_json_dict(cmd.args)

        # Try metaplugins first
        if await oi.metaplugin_loader.call_handle_custom_command_hooks(cmd.cmd, args, cmd.ses, oi):
            return clap.Empty()

        if cmd.cmd == "new_folder":
            parent_folder = (await oi.folders_helper.get_current_folder_path(cmd.ses, None))[-1]
            # Create folder & refresh user's view
            if new_folder_name := args.get("name"):
                with oi.db_new_session() as dbs:
                    new_fld = await oi.folders_helper.create_folder(dbs, cmd.ses, parent_folder, new_folder_name)
                oi.log.debug(f"Folder {new_fld.id} ('{new_fld.title}') created & committed, refreshing client's page")
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(parent_folder.id, exclude_sid=cmd.ses.sid)
            else:
                oi.log.error("new_folder command missing 'name' argument")
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "new_folder command missing 'name' argument")

        elif cmd.cmd == "open_folder":
            # Validate & parse argument JSON
            open_args = parse_json_dict(cmd.args)
            assert isinstance(open_args, dict), "open_folder argument not a dict"
            folder_id = open_args.get("id")
            assert folder_id, "open_folder arg 'id' missing"
            assert isinstance(folder_id, int), "open_folder arg 'id' not an int"

            # Check if target folder exists and get its owner
            with oi.db_new_session() as dbs:
                target_folder = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not target_folder:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")
                target_owner_id = target_folder.user_id

            # Construct new breadcrumb trail
            folder_path, _root_folder = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
            trail = [f.id for f in folder_path]

            if folder_id in trail:
                # Going back up in current trail => remove all after this folder
                trail = trail[:trail.index(folder_id)+1]
            else:
                # Check if we're crossing ownership boundaries
                current_owner_id = folder_path[-1].user_id if folder_path else None

                if (current_owner_id != target_owner_id and
                    target_owner_id != cmd.ses.user.id and
                    cmd.ses.is_admin):
                    # Admin is switching to another user's folder - start fresh trail from target
                    oi.log.debug(f"Admin switching from {current_owner_id} to {target_owner_id} folders - starting fresh trail")
                    trail = [folder_id]
                else:
                    # Normal case: append folder id at the end
                    trail.append(folder_id)

            # Update folder path cookie
            serialized_trail = json.dumps(trail)
            cmd.ses.cookies[PATH_COOKIE_NAME] = serialized_trail
            oi.log.debug(f"Setting new folder_path cookie: {serialized_trail}")
            await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(
                cookies = cmd.ses.cookies,
                sid = cmd.ses.sid))

            # Update page to view the opened folder
            page = await oi.pages_helper.construct_navi_page(cmd.ses, serialized_trail)
            await oi.srv.client_show_page(page)

        elif cmd.cmd == "rename_folder":
            args = parse_json_dict(cmd.args)  # {"id": 123, "new_name": "New name"}
            if not args or not args.get("id") or not args.get("new_name"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "rename_folder command missing 'id' or 'new_name' argument")
            folder_id = int(args["id"])
            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{args['id']}' not found")

                # Check authorization via metaplugins + default checks
                from .authz_methods import check_action_authorization
                await check_action_authorization(oi, "rename_folder", folder=fld, ses=cmd.ses)

                with dbs.begin_nested():
                    fld.title = args["new_name"]

            oi.log.debug(f"Renamed folder '{fld.id}' to '{fld.title}'")
            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(fld.id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "trash_folder":
            args = parse_json_dict(cmd.args) # {"id": 123}
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "trash_folder command missing 'id' argument")
            folder_id = int(args["id"])

            # Get folder and check authorization
            from .authz_methods import check_action_authorization
            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")
                await check_action_authorization(oi, "trash_folder", folder=fld, ses=cmd.ses)

            # Remember parent folder for notifying other viewers
            folder_path, _ = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
            parent_folder_id = folder_path[-2].id if len(folder_path) > 1 else folder_path[-1].id

            # Delete the folder and its contents, gather media file IDs to delete later (after transaction, to avoid DB locks)
            media_to_delete = []
            with oi.db_new_session() as dbs:
                with dbs.begin_nested():
                    media_to_delete = await oi.folders_helper.trash_folder_recursive(dbs, folder_id, cmd.ses)

            # Trash the media files
            for vi in media_to_delete:
                oi.log.debug(f"Trashing media file '{vi}'")
                await oi.srv.delete_media_file(org.DeleteMediaFileRequest(id=vi))  # this cleans up the media's files on disk, too

            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(parent_folder_id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "share_folder":
            args = parse_json_dict(cmd.args)
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "share_folder command missing 'id' argument")

            folder_id = int(args["id"])

            with oi.db_new_session() as dbs:
                shared = await oi.folders_helper.create_folder_share(dbs, folder_id, cmd.ses)
                folder_title = str(shared.folder.title)
                dbs.commit()

                # Generate shareable URL using server_url_base from server_info
                if not oi.server_info or not oi.server_info.url_base:
                    raise GRPCError(GrpcStatus.FAILED_PRECONDITION, "Server URL base not configured - cannot generate shareable URLs")

            # Update UI after transaction commit
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

            # Show message with share URL
            await try_send_user_message(oi.srv,
                org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                    msg=clap.UserMessage(
                        message="Dossier partagé. Utilisez le menu contextuel / 'Copier l'URL' pour obtenir le lien.",
                        details= f"Lien de partage créé pour '{folder_title}'.",
                        type=clap.UserMessageType.OK)))

        elif cmd.cmd == "revoke_share":
            # Parse arguments
            args = parse_json_dict(cmd.args)
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "revoke_share command missing 'id' argument")

            folder_id = int(args["id"])

            with oi.db_new_session() as dbs:
                # Revoke the share
                revoked = await oi.folders_helper.revoke_folder_share(dbs, folder_id, cmd.ses)
                dbs.commit()

            # Update UI after transaction commit
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

            # Show success message
            if revoked:
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message="Le partage du dossier a été révoqué",
                            type=clap.UserMessageType.OK)))
            else:
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message="Ce dossier n'est pas actuellement partagé",
                            type=clap.UserMessageType.ERROR)))

        elif cmd.cmd == "cleanup_empty_user":
            # Parse arguments
            args = parse_json_dict(cmd.args)
            if not args or not args.get("folder_id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "cleanup_empty_user command missing 'folder_id' argument")

            if not cmd.ses.is_admin:
                raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Only admin can clean up users")

            folder_id_str = str(args["folder_id"])

            # Check if this is a batch cleanup request (folder_id = '*')
            if folder_id_str == "*":
                # Batch cleanup all empty users (excluding the admin user who is performing the action)
                cur_folder_id = (await oi.folders_helper.get_current_folder_path(cmd.ses, None))[-1].id
                with oi.db_new_session() as dbs:
                    from .folder_op_methods import find_and_cleanup_empty_users
                    cleaned_count = await find_and_cleanup_empty_users(dbs, oi.log, exclude_user_id=cmd.ses.user.id)
                    dbs.commit()

                # Update UI after transaction commit
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(cur_folder_id, exclude_sid=cmd.ses.sid)

                # Show result message
                if cleaned_count > 0:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=f"{cleaned_count} utilisateur{'s' if cleaned_count != 1 else ''} supprimé{'s' if cleaned_count != 1 else ''}",
                                details="Les commentaires des utilisateurs supprimés sont conservés mais marqués comme tels.",
                                type=clap.UserMessageType.OK)))
                else:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message="Aucun utilisateur vide trouvé à supprimer",
                                type=clap.UserMessageType.OK)))
            else:
                # Single user cleanup (existing logic)
                folder_id = int(folder_id_str)

                # Find the user who owns this folder
                with oi.db_new_session() as dbs:
                    target_folder = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                    if not target_folder:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")

                    user_id = target_folder.user_id
                    from .folder_op_methods import _cleanup_empty_user
                    was_deleted = await _cleanup_empty_user(dbs, user_id, oi.log)
                    dbs.commit()

                # Update UI after transaction commit
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

                # Show result message
                if was_deleted:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=f"L'utilisateur '{user_id}' a été supprimé",
                                details="Les commentaires de cet utilisateur sont conservés mais marqués comme provenant d'un utilisateur supprimé.",
                                type=clap.UserMessageType.OK)))
                else:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=f"L'utilisateur '{user_id}' n'a pas été supprimé",
                                details="L'utilisateur a encore des fichiers médias ou des dossiers non vides.",
                                type=clap.UserMessageType.OK)))

        elif cmd.cmd == "set_user_email":
            if not cmd.ses.is_admin:
                raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Only admin can set user email addresses")

            args = parse_json_dict(cmd.args)
            user_id = args.get("user_id")
            email = (args.get("email") or "").strip()

            if not user_id:
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "set_user_email command missing 'user_id' argument")

            from .database.models import DbUser
            with oi.db_new_session() as dbs:
                user = dbs.query(DbUser).filter(DbUser.id == user_id).one_or_none()
                if not user:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"User '{user_id}' not found")
                with dbs.begin_nested():
                    user.email = email if email else None
                dbs.commit()

            oi.log.info(f"Admin set email for user '{user_id}' to '{email or '(cleared)'}'")

            # Refresh the admin view
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)

            await try_send_user_message(oi.srv,
                org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                    msg=clap.UserMessage(
                        message=f"Email de '{user_id}' mis à jour" if email else f"Email de '{user_id}' effacé",
                        type=clap.UserMessageType.OK)))

        elif cmd.cmd == "link_as_version":
            args = parse_json_dict(cmd.args)
            primary_id = (args.get("primary_id") or "").strip()
            version_id = (args.get("version_id") or "").strip()

            if not primary_id or not version_id:
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "link_as_version: primary_id et version_id requis")
            if primary_id == version_id:
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Une vidéo ne peut pas être sa propre version")

            # Vérifier que les deux fichiers existent et que l'utilisateur y a accès
            from .database.models import DbMediaFile as DbMF
            with oi.db_new_session() as dbs:
                primary = dbs.query(DbMF).filter(DbMF.id == primary_id).one_or_none()
                version = dbs.query(DbMF).filter(DbMF.id == version_id).one_or_none()

                if not primary:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Vidéo principale '{primary_id}' introuvable")
                if not version:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Vidéo '{version_id}' introuvable")
                if primary.version_of:
                    raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "La vidéo principale est elle-même une version — choisissez la version principale du groupe")
                if not cmd.ses.is_admin and primary.user_id != cmd.ses.user.id:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Vous ne pouvez lier que vos propres vidéos")

                with dbs.begin_nested():
                    version.version_of = primary_id
                dbs.commit()

            oi.log.info(f"Lié '{version_id}' comme version de '{primary_id}'")
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await try_send_user_message(oi.srv,
                org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                    msg=clap.UserMessage(
                        message=f"Version liée avec succès",
                        type=clap.UserMessageType.OK)))

        elif cmd.cmd == "unlink_version":
            args = parse_json_dict(cmd.args)
            version_id = (args.get("version_id") or "").strip()
            if not version_id:
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "unlink_version: version_id requis")

            from .database.models import DbMediaFile as DbMF
            with oi.db_new_session() as dbs:
                version = dbs.query(DbMF).filter(DbMF.id == version_id).one_or_none()
                if not version:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Vidéo '{version_id}' introuvable")
                if not cmd.ses.is_admin and version.user_id != cmd.ses.user.id:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Accès refusé")
                with dbs.begin_nested():
                    version.version_of = None
                dbs.commit()

            oi.log.info(f"Délié la version '{version_id}'")
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await try_send_user_message(oi.srv,
                org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                    msg=clap.UserMessage(
                        message="Version dissociée — la vidéo est de nouveau indépendante",
                        type=clap.UserMessageType.OK)))

        else:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, f"Unknown organizer command: {cmd.cmd}")

    except GRPCError as e:

        # Intercept some known session errors and show them to the user nicely
        if e.status in (GrpcStatus.INVALID_ARGUMENT, GrpcStatus.PERMISSION_DENIED, GrpcStatus.ALREADY_EXISTS):
            if err := await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message=str(e.message),
                            user_id=cmd.ses.user.id,
                            type=clap.UserMessageType.ERROR,
                            details=str(e.details) if e.details else None))):
                oi.log.error(f"Error calling client_show_user_message(): {err}")
        else:
            raise e

    return clap.Empty()
