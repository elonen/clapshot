# "Basic Folders" Organizer plugin for Clapshot

Implements a basic folder tree for users to organize their videos into.
Also serves as an example implementation and a test bench for the Clapshot Organizer API.

Written in Python (due to popularity), but Organizers can be implemented in any language that supports gRPC.

Technically, it:

- Adds to SQlite `bf_folders` and `bf_folder_items` tables, and links them to the Clapshot Server's `videos` and `users` tables with foreign keys:
  - `bf_folders` describes the folders
  - `bf_folder_items` lists which videos or (sub)folders are in which folder
- Auto-creates home folders to all users who currently don't have them, and adds `bf_folder_items` entries to it for any danglig video objects
- Intercepts Clapshot Client's requests for a navigation page, and instead of simple video listing (like Server does by default), shows navigable folders. In addition to the DB-baacked folders, it shows virtual folders for admins to manage other users' videos.
- Posts the Client some Javascript callbacks for folder and video UI items, so that users can create, rename, reorder and trash both videos and folders using popup menus and drag-and-drop.

This demonstrates how you can create arbitrary UI folder hierarchies, inject custom HTML + JS code to the Client's navigation window, and how to extend the database schema to store custom plugin data.

## License (MIT)

Released under the **MIT license**, unlike the main server (which is GPLv2) so you can use this as a basis for custom, proprietary extensions.
