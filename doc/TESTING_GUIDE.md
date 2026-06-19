# Clapshot Testing Guide
Compiled with the help of Claude Code

This document provides instructions for testing Clapshot features. It covers both automated testing and manual testing procedures for developers and contributors.

## Table of Contents

1. [Quick Start Testing](#quick-start-testing)
2. [Architecture Overview](#architecture-overview)
3. [Prerequisites](#prerequisites)
4. [Automated Testing](#automated-testing)
5. [Manual Testing](#manual-testing)
6. [Testing Organizer Plugins](#testing-organizer-plugins)
7. [CI/CD Testing](#cicd-testing)
8. [Troubleshooting](#troubleshooting)

## Quick Start Testing

### Full Stack Test (Recommended)
```bash
# Run all tests in Docker containers (most reliable)
make test
```

This command:

1. Builds Docker images for all components
2. Runs client build tests
3. Runs server unit and integration tests
4. Runs organizer plugin tests
5. Tests complete system integration

### Component-Specific Quick Tests
```bash
# Client only
cd client && make test-docker

# Server only
cd server && make test-docker

# Organizer only
cd organizer/basic_folders && make install
```

## Architecture Overview

### System Components

Clapshot is a multi-component system designed for collaborative video/media review and annotation.

#### Core Components

**1. Clapshot Server (Rust)**

- **Location**: `server/`
- **Technology**: Rust with Tokio async runtime
- **Responsibilities**:
  - WebSocket API for real-time client communication
  - Media file ingestion and processing pipeline
  - Video transcoding with FFmpeg
  - Thumbnail and preview generation
  - SQLite database management with Diesel ORM
  - gRPC server for organizer plugin communication
  - Authentication integration via HTTP headers
- **Key Dependencies**:
  - `diesel` (database ORM)
  - `warp` (HTTP/WebSocket server)
  - `tonic` (gRPC framework)
  - `tokio` (async runtime)

**2. Clapshot Client (Svelte)**

- **Location**: `client/`
- **Technology**: Svelte with TypeScript, built with Vite
- **Responsibilities**:
  - Single Page Application (SPA) for browser UI
  - Real-time WebSocket communication with server
  - Video player with annotation tools
  - File upload interface
  - Folder navigation and management
- **Key Dependencies**:
  - `svelte` (UI framework)
  - `tailwindcss` (styling)
  - `flowbite-svelte` (UI components)
  - `vite` (build tool)

**3. Organizer Plugins (Python/Others)**

- **Location**: `organizer/basic_folders/` (example implementation)
- **Technology**: Python with gRPC, extensible to any language
- **Responsibilities**:
  - Custom folder hierarchies and virtual folders
  - User access control and permission management
  - Database schema extensions
  - Custom UI injection via HTML/JavaScript
  - Integration with external systems (LDAP, project management, etc.)
- **Key Dependencies**:
  - `grpclib` (gRPC client/server)
  - `SQLAlchemy` (database ORM)
  - `betterproto` (protobuf code generation)

#### Supporting Infrastructure

**4. Protocol Buffers (gRPC)**

- **Location**: `protobuf/`
- **Technology**: Protocol Buffers with language-specific bindings
- **Responsibilities**:
  - Inter-service communication contracts
  - Type-safe API definitions
  - Language-agnostic data serialization
- **Generated Libraries**:
  - Rust: `protobuf/libs/rust/`
  - Python: `protobuf/libs/python/`
  - TypeScript: `protobuf/libs/typescript/`

**5. Database Layer**

- **Technology**: SQLite with migrations
- **Schema Management**: Diesel migrations (Rust) + custom organizer migrations
- **Key Tables**:
  - `media_files` - Core media metadata
  - `comments` - User annotations and discussions
  - `users` - User authentication data
  - `bf_folders`, `bf_folder_items` - Basic folders plugin schema
  - `bf_shared_folders` - Folder sharing functionality

**6. Media Processing Pipeline**

- **Tools**: FFmpeg, mediainfo
- **Processes**:
  - Format detection and metadata extraction
  - Video/audio transcoding for web compatibility
  - Thumbnail and preview sheet generation
  - Subtitle track processing

### Deployment Architecture

**Development Mode:**

```
Browser ← WebSocket → Clapshot Server ← gRPC → Organizer Plugin
   ↑                        ↓
HTTP/Static Files      SQLite Database
                            ↓
                    Media Files + FFmpeg
```

**Production Mode:**

```
Browser ← HTTPS → Nginx ← HTTP → Clapshot Server ← gRPC → Organizer Plugin
                    ↓              ↓                         ↓
              Auth Proxy    SQLite Database          Custom Integrations
                              ↓
                    Media Files + FFmpeg
```

### Testing Architecture

The testing strategy reflects the component architecture:

**1. Unit Testing**

- **Server**: Rust `cargo test` with database mocking
- **Client**: TypeScript compilation and build verification
- **Organizer**: Python unit tests via gRPC interface

**2. Integration Testing**

- **Full Stack**: Docker containers with real databases
- **Media Pipeline**: End-to-end file processing tests
- **gRPC Communication**: Server-organizer interaction tests

**3. System Testing**

- **Docker Compose**: Complete multi-container deployment
- **Load Testing**: Media processing under concurrent load
- **Migration Testing**: Database upgrade scenarios

### Key Design Patterns

**Event-Driven Architecture**

- WebSocket events for real-time UI updates
- File system watching for automatic media ingestion
- Database triggers for audit logging

**Plugin Architecture**

- gRPC-based plugin system for extensibility
- Database schema evolution through migrations
- HTML/JavaScript injection for custom UIs

**Microservice Communication**

- Type-safe gRPC contracts
- Async/await throughout the stack
- Graceful error handling and recovery

### Technology Choices Rationale

**Rust for Server:**

- Memory safety for long-running media processing
- Excellent async performance for WebSocket handling
- Strong type system for API contracts

**Svelte for Client:**

- Minimal runtime overhead for smooth video playback
- Reactive updates for real-time collaboration
- TypeScript integration for maintainability

**Python for Organizer:**

- Rapid prototyping of business logic integrations
- Rich ecosystem for database and API integrations
- Familiar language for system administrators

**SQLite for Database:**

- Single-file deployment simplicity
- ACID transactions for data consistency
- Sufficient performance for typical use cases

This architectural understanding is essential for writing effective tests that validate both individual component behavior and system-wide integration.

## Prerequisites

### Required Tools
- **Docker** (for containerized testing)
- **Rust** (stable toolchain via rustup)
- **Node.js** (for client testing)
- **Python 3** (for organizer plugin testing)
- **Make** (for build automation)
- **jq** (for JSON processing in scripts)
- **FFmpeg** and **mediainfo** (for media processing tests)

### Platform Support
- **Primary**: Linux (Ubuntu/Debian)
- **Development**: macOS, Windows WSL2
- **CI**: GitHub Actions on Ubuntu

## Automated Testing

### Server Testing

#### Unit Tests
```bash
cd server

# Local testing (requires Rust toolchain)
make test-local

# Docker testing (isolated environment)
make test-docker
```

**Test Types:**

- Database operations and migrations
- Video/audio/image processing pipeline
- WebSocket API endpoints
- gRPC organizer communication
- File upload and transcoding

#### Integration Tests
Located in `server/src/tests/integration_test.rs`, these tests:
- Test complete video ingestion workflow
- Verify transcoding with different formats
- Test WebSocket client communication
- Validate organizer plugin interaction

**Example Test Scenarios:**

- MP4 ingestion without transcoding: `test_video_ingest_no_transcode`
- MOV transcoding: `test_video_mov_ingest_and_transcode`
- Audio file processing: `test_audio_ingest_and_transcode`
- Database migration: `test_existing_v056_migrate_and_image_ingest`

### Client Testing

```bash
cd client

# Type checking and build
npm run check
npm run build

# Development server
npm run dev
```

**Test Coverage:**

- TypeScript compilation
- Svelte component compilation
- Build process validation
- Asset bundling

### Organizer Plugin Testing

#### Basic Folders Plugin
```bash
cd organizer/basic_folders

# Install dependencies and run type checking
make install

# Run all organizer tests through server
cd ../../server
TEST_ORG_CMD="../organizer/basic_folders/run-py-org.sh" make test-local
```

**Test Categories:**
- Database schema creation and migration
- Folder operations (create, rename, move, delete)
- File organization and ownership transfer
- User session management
- Sharing functionality
- Permission validation

#### Custom Organizer Testing
For your own organizer plugins:

1. **Implement Test Interface:**
   ```python
   async def list_tests_impl(oi) -> org.ListTestsResponse:
       # Return list of test function names starting with 'org_test_'

   async def run_test_impl(oi, request: org.RunTestRequest) -> org.RunTestResponse:
       # Execute individual tests by name
   ```

2. **Create Test Functions:**
   ```python
   async def org_test__your_feature(oi: organizer.OrganizerInbound):
       # Your test implementation
       assert condition, "Test failure message"
   ```

3. **Run Tests:**
   ```bash
   TEST_ORG_CMD="path/to/your/organizer/run-script.sh" make test-local
   ```

## Manual Testing

### Local Development Setup

#### Option 1: Mixed Local/Docker
```bash
# Terminal 1: Server (local Rust)
cd server
make run-local

# Terminal 2: Client (local Node.js)
cd client
npm run dev

# Access: http://localhost:5173/
```

#### Option 2: Full Docker
```bash
# Build and run complete stack
make run-docker

# Access: http://localhost:8080/
```

### Testing Workflows

#### Media Upload and Processing
1. **Upload Test Files:**
   - Copy test media to `server/DEV_DATADIR/incoming/`
   - Or use web upload interface
   - Test files available in `server/src/tests/assets/`

2. **Verify Processing:**
   - Check transcoding completion
   - Verify thumbnail generation
   - Test playback in browser

#### Organizer Plugin Features

##### Folder Management
1. Create folders in the UI
2. Drag and drop files between folders
3. Test folder sharing functionality
4. Verify permission controls

##### Admin Features (if applicable)
1. Test user management
2. Verify cross-user folder operations
3. Test ownership transfer

### Production-like Testing

#### Debian Package Testing
```bash
# Build .deb packages
make debian-docker

# Install and test packages
sudo dpkg -i dist_deb/clapshot-*.deb
sudo systemctl start clapshot-server
```

#### Demo Container Testing
```bash
# Single-user demo
docker run --rm -it -p 8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo

# Multi-user demo with auth
docker run --rm -it -p 8080:80 -v clapshot-demo-htwicket:/mnt/clapshot-data/data elonen/clapshot:latest-demo-htwicket
```

#### Cloudflare Tunnel Testing
```bash
# Test internet-accessible deployment
./test/run-cloudflare.sh
```

## Testing Organizer Plugins

### Test Structure
Organizer tests are written in Python and executed via gRPC by the server. This ensures realistic testing of the complete plugin interface.

### Key Test Areas

#### Database Integration
- Schema creation and migration
- Foreign key relationships
- Transaction handling

#### User Session Management
- Authentication and authorization
- Cookie handling
- Admin vs regular user permissions

#### Folder Operations
```python
# Example test pattern
async def org_test__folder_operation(oi):
    # Setup test data
    user = create_test_user()
    folder = create_test_folder(user)

    # Execute operation
    result = await oi.some_operation(params)

    # Verify results
    assert result.success
    verify_database_state()
```

#### Error Handling
- Permission denied scenarios
- Invalid input validation
- Concurrent operation handling

### Test Data Management
- Tests use temporary databases
- Automatic cleanup after test completion
- Isolation between test runs

## CI/CD Testing

### GitHub Actions
See `.github/workflows/docker-test.yml`:

```yaml
- name: Build and test
  run: make test
```

**CI Environment:**
- Ubuntu latest
- Docker-based testing
- Parallel component builds
- Automated on all branches and PRs

### Continuous Integration Best Practices
1. **All tests must pass** before merging
2. **Docker-based testing** ensures consistency
3. **Slow tests** are feature-gated for CI (`include_slow_tests`)
4. **Logs are captured** for debugging failures

## Troubleshooting

### Common Issues

#### Test Timeouts
```bash
# Increase timeout for slow systems
RUST_TEST_TIMEOUT=300 cargo test
```

#### Docker Permission Issues
```bash
# On Linux, ensure correct user permissions
sudo chown -R $USER:$USER dist_deb/
```

#### gRPC Connection Issues
```bash
# Check if organizer binary is executable
chmod +x organizer/basic_folders/run-py-org.sh

# Verify Python virtual environment
cd organizer/basic_folders && make install
```

#### Media Processing Failures
- Ensure FFmpeg and mediainfo are installed
- Check that test media files are present
- Verify sufficient disk space for transcoding

### Debug Mode Testing
```bash
# Server with debug logging
cd server
make run-local RUST_LOG=debug

# Organizer with debug logging
cd organizer/basic_folders
_venv/bin/python -m organizer.main --debug /tmp/test.sock
```

### Test Asset Management
Test media files are stored in `server/src/tests/assets/`:

- `60fps-example.mp4` - MP4 video (no transcoding needed)
- `NASA_Red_Lettuce_excerpt.mov` - MOV video (requires transcoding)
- `Apollo11_countdown.mp3` - Audio file
- `NASA-48410_PIA25967_-_MAV_Test.jpeg` - Image file
- Various subtitle formats (SRT, VTT, ASS)

### Performance Testing
For performance-sensitive features:

1. Use release builds: `cargo build --release`
2. Profile with appropriate tools
3. Test with realistic media file sizes
4. Monitor resource usage during tests
