# Example "organizer" Python hook script.
#
# At its core, Clapshot keeps videos in a flat list. Python hooks can
# be used to organize them into a custom hierarchy, enforce project-based
# visibility, permissions etc.
#
# This default/example script implements a basic projects-and-folders organization.

#from ui_types import Action, Dialog, DialogType

# Clapshot API is exposed to Python via this object:
clap = clapshot_native  # type: ignore
assert clap

clap.log("Default organizer script loading")


"""
# Define actions that user can perform on UI elements,
# and the corresponding API commands to execute.
ACTION_DEFINITIONS = [

    # For folder path breadcrumbs
    Action(
        id='open_path',
        api_cmd = 'open_path',
        api_data = { 'path': '$ARGS.path' }
    ),

    # Video popup menu actions
    # ------------------------

    Action(
        id='rename_video',
        label='Rename',
        key_shortcut='F2',
        icon_class='fa fa-edit',
        dlg = Dialog(
            type = DialogType.INPUT_LINE,
            title = 'Rename video',
            intro = 'Enter new name for video "$ARGS.initial":',
            args = {
                'value': '$ARGS.initial',
            }
        ),
        exec_if = '$DLG.ok',
        api_cmd = 'organizer',
        api_data = {
            'subcmd': 'rename',
            'data': {
                'video': '$ITEM.id',
                'new_name': '$DLG.text',
            }
        }
    ),

    Action(
        id='trash_video',
        label='Trash',
        key_shortcut='Delete',
        icon_class='fa fa-trash',
        dlg = Dialog(
            type = DialogType.YES_NO,
            title = 'Trash video?',
            intro = 'Really move "$ITEM.label" into trash?',
            args = {}
        ),
        exec_if = '$DLG.yes',
        api_cmd = 'organizer',
        api_data = {
            'subcmd': 'trash',
            'data': {
                'video': '$ITEM.id',
            }
        }
    ),

    # Folder popup menu actions
    # -------------------------

    Action(
        id='rename_folder',
        label='Rename',
        key_shortcut='F2',
        icon_class='fa fa-edit',
        dlg = Dialog(
            type = DialogType.INPUT_LINE,
            title = 'Rename folder',
            intro = 'Enter new name for folder "$ARGS.initial":',
            args = {
                'value': '$ARGS.initial',
            }
        ),
        exec_if = '$DLG.ok',
        api_cmd = 'organizer',
        api_data = {
            'subcmd': 'rename',
            'data': {
                'folder': '$ITEM.id',
                'new_name': '$DLG.text',
            }
        }
    ),

    Action(
        id='trash_folder',
        label='Trash',
        key_shortcut='Delete',
        icon_class='fa fa-trash',
        dlg = Dialog(
            type = DialogType.YES_NO,
            title = 'Trash folder?',
            intro = 'Really move "$ITEM.label" and ALL CONTENTS into trash?',
            args = {}
        ),
        exec_if = '$DLG.yes',
        api_cmd = 'organizer',
        api_data = {
            'subcmd': 'trash',
            'data': {
                'folder': '$ITEM.id',
            }
        }
    ),
]
"""
