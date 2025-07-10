# Clapshot Architecture Overview

This document explains how Clapshot's components communicate during a typical user session. Understanding this flow is crucial for troubleshooting connection issues and configuring deployments correctly.

## Architecture Components

Clapshot consists of several interconnected components:

- **Browser**: User's web browser running the Clapshot client (Svelte SPA)
- **Client**: JavaScript application running in the browser
- **Nginx**: Web server and reverse proxy
- **Server**: Rust backend server (listening on port 8095)
- **Organizer**: Python process that manages video content and user permissions
- **Database**: SQLite database for storing video metadata and user data
- **Authentication**: Authentication system (can be HTTP basic auth or external)
- **Filesystem**: Storage for video files and thumbnails

## Communication Flow

A typical Clapshot session involves multiple phases of communication between these components:

### Phase 1: Initial Page Load

1. **Browser → Nginx**: User navigates to Clapshot URL (HTTPS GET /)
2. **Nginx ↔ Filesystem**: Nginx reads the `index.html` file from disk
3. **Nginx → Browser**: Returns the main HTML page (encrypted via HTTPS)
4. **Browser → Client**: Browser executes JavaScript to start the Clapshot client
5. **Client → Browser**: Client generates and displays the user interface HTML
6. **Browser → Nginx**: Browser requests additional assets (HTTPS GET for JS, CSS, images)
7. **Nginx ↔ Filesystem**: Nginx reads static asset files from disk
8. **Nginx → Browser**: Returns static assets (encrypted via HTTPS)

### Phase 2: WebSocket Session Initialization

1. **Client → Nginx**: Client requests configuration file (HTTPS GET `/clapshot_client.conf.json`)
2. **Nginx ↔ Filesystem**: Nginx reads the client configuration file
3. **Nginx → Client**: Returns configuration JSON containing WebSocket URL and other settings
4. **Client (internal)**: Client parses the WSS_URL from the configuration
5. **Client → Nginx**: Client initiates WebSocket connection (Connect wss://WSS_URL)
6. **Nginx → Authentication**: Nginx forwards authentication/authorization request
7. **Authentication → Nginx**: Authentication system returns HTTP 200 OK with user ID
8. **Nginx → Server**: Nginx proxies WebSocket connection to local server (ws://127.0.0.1:8095)
9. **Server → Client**: Server sends welcome message via protobuf over encrypted WebSocket

### Phase 3: Interaction with Organizer and Database

1. **Server → Organizer**: Server calls gRPC `on_start_user_session()` to initialize user session
2. **Organizer → Server**: Organizer responds with gRPC `client_define_actions` (available user actions)
3. **Server → Client**: Server sends `DefineActions` message via protobuf over WebSocket
4. **Client → Server**: Client requests video list (`ListMyVideos` via protobuf over WebSocket)
5. **Server → Organizer**: Server calls gRPC `navigate_page()` to get user's videos
6. **Organizer ↔ Database**: Organizer queries SQLite database for video metadata
7. **Organizer → Server**: Organizer returns gRPC `client_show_page()` with video list
8. **Server → Client**: Server sends `ShowPage` message with video data via protobuf
9. **Client → Browser**: Client updates browser display to show video list

### Phase 4: Thumbnail Retrieval

1. **Browser → Nginx**: Browser requests thumbnail images (HTTPS GET for each thumbnail)
2. **Nginx → Authentication**: Nginx validates authentication/authorization for image URLs
3. **Authentication → Nginx**: Authentication system returns 200 OK for authorized requests
4. **Nginx ↔ Filesystem**: Nginx reads thumbnail image files from disk
5. **Nginx → Browser**: Returns thumbnail images (encrypted via HTTPS)

### Phase 5: Video Playback

1. **Browser → Client**: User clicks on a video to open it
2. **Client → Browser**: Client executes the `PageItem.open_action` provided by the Organizer
3. **Client → Server**: Client sends `OpenVideo` request with video ID via protobuf over WebSocket
4. **Server → Organizer**: Server calls gRPC `authz_user_action()` to verify user permissions
5. **Organizer → Server**: Organizer confirms authorization
6. **Server → Client**: Server sends `OpenVideo` response with video URL and comments via protobuf
7. **Client → Browser**: Client creates HTML5 video element and sets video source
8. **Browser → Nginx**: Browser requests video file (HTTPS GET for video stream)
9. **Nginx → Authentication**: Nginx validates authentication/authorization for video file access
10. **Authentication → Nginx**: Authentication system returns 200 OK for authorized requests
11. **Nginx ↔ Filesystem**: Nginx streams video file from disk
12. **Nginx → Browser**: Streams video content to browser (encrypted via HTTPS)

## Configuration Impact on Communication

Different configuration options affect specific parts of this communication flow:

### Client Configuration (`/etc/clapshot_client.conf` or `clapshot_client.conf.json`)

- **`ws_url`**: Determines the WebSocket URL used in Phase 2, step 5
- **`upload_url`**: Sets the endpoint for file uploads (not shown in basic flow)
- **Base URL components**: Must match the server's externally accessible address

### Docker Environment Variables

- **`CLAPSHOT_URL_BASE`**: Docker startup script uses this to automatically generate the client configuration file
- **`CLAPSHOT_CORS`**: Configures Cross-Origin Resource Sharing policies for the nginx server

### Server Configuration

- **Listen port (8095)**: Internal port where the Rust server accepts connections from nginx
- **gRPC settings**: Configure communication with the Organizer process

### Nginx Configuration

- **Reverse proxy rules**: Route WebSocket and API requests to the backend server
- **Static file serving**: Serve client assets and media files
- **Authentication integration**: Handle user authentication before proxying requests

## Common Points of Failure

Understanding this flow helps identify where problems typically occur:

1. **Phase 1 failures**: Usually indicate nginx configuration or static file issues
2. **Phase 2 failures**: Often caused by incorrect client configuration or WebSocket proxy issues
3. **Phase 3 failures**: May indicate server startup problems or Organizer communication issues
4. **Phase 4/5 failures**: Often related to authentication/authorization or file access permissions

Each phase depends on the successful completion of previous phases, so troubleshooting should start from the beginning of the flow.