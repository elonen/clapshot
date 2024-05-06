# Organizer Plugins

Clapshot 0.6.0 introduces an extensible "Organizer" plugin system. The API currently **experimental / unstable**.

## Overview

An Organizer plugin can:
1. Organize Clapshot videos into (user defined and virtual) folders
2. Enforce access control according to your particular business logic and backend systems
3. Implement custom workflows and UI elements (raw HTML + Javascrip callbacks)

You might, for example, look up projects and ACL groups from an LDAP directory or project management database to determine folder structure and user permissions.


## The `basic_folders` Organizer

Clapshot 0.6.0 includes a basic example Organizer plugin called `basic_folders`, written in Python. Without an Organizer plugin, the Clapshot server will send the client (web UI) a flat list of all the user's videos, without any hierarchy or folder structure. While the client is technically capable of displaying folders, it relies on the server to provide the necessary UI markup.

The `basic_folders` plugin demonstrates how the Organizer system can be used to introduce a folder hierarchy. It allows users to organize their videos into a tree-like structure of folders. It doesn't implement any additional access control -- any user who knows the hash id for someone else's video can still watch and comment on that video.

The reason folder functionality isn't built directly into the main Clapshot server module is twofold:

1. If someone wants to implement a custom virtual folders (e.g., views to an external storage, corporate project folders, etc.), a built-in organization logic would interfere with that.

2. By implementing folders as a separate plugin, the Clapshot developer can ensure that the plugin API is powerful and flexible enough to support things like these.

Thus, the `basic_folders` plugin serves both as a practical UI enhancement for basic usage, and also as proof-of-concept and a starting point for developers who want to create more sophisticated Organizer plugins tailored to their specific needs.


## Architecture

In startup, the Clapshot server connects to the Organizer plugin (*max one in current version, support for multiple organizers coming up*) using gRPC protocol, and sends a handshake message. The Organizer plugin then connects back to the server and sends its own handshake message, to establish a bidirectional communication.

The Organizer gRPC service must remain running as long as the Clapshot server is running and maintain a stable connection. It is recommended to have the server launch the Organizer plugin as a subprocess and use **Unix sockets** for communication, but TCP is also possible.

Organizer plugins can have their own SQL migrations, which the server will run when necessary. The idea is that they will share the SQLite database with the server so they can utilize foreign keys, triggers, etc.


## API

The gRPC API for Organizer plugins is defined in `organizer.proto`. It includes two main services:

1. `OrganizerOutbound`: Calls that the Organizer makes to the Clapshot server
2. `OrganizerInbound`: Calls that the Clapshot server makes to the Organizer

Some key methods for `OrganizerInbound` include:

- `handshake`: For initial connection setup
- `on_start_user_session`: Called when a new user session starts
- `navigate_page`: Called when the user navigates to a new page
- `authz_user_action`: Called to authorize user actions
- `move_to_folder` / `reorder_items`: Called when user interacts with the folder UI


## Development

**WARNING -- Please note that the Organizer API is new, still evolving and may change in future releases. Developers are encouraged to provide feedback and express their wishes for further development.**

Rough steps to develop an Organizer plugin:

1. Generate gRPC bindings from `organizer.proto` for your language of choice (or use the provided libs)
2. Implement the `OrganizerInbound` service
3. Connect to the Clapshot server's `OrganizerOutbound` service
4. Deploy your plugin alongside the Clapshot server

There are scripts to generate gRPC bindings for Rust, Python, and TypeScript in the `protobuf/libs` directory. In principle, organizers can be implemented in any language thanks to gRPC.


## Future Development

The next versions of the Organizer API will likely introduce:

1. Support for multiple organizers (ran in sequence, offering first ones the opportunity to either handle and stop the cascade, or to pass the request along to the next one)
2. Support for custom video ingestion (e.g. converting plain audio files into videos with waveforms, to allow review of audio-only clips)
3. For `basic_folders`, a virtual folder view of every user's videos in the system for the `admin` user
