## Development setup

This is my current development setup (in spring 2023). Adapt to your own needs.

 * Windows with WSL2, running Debian Bullseye
 * Docker (running inside the WSL2 Debian)

First, open two WSL2 Debian terminals open - one for server dev (Rust), one for client dev (Svelte). Then:

**Server**:

 * install rustup 2021 stable toolchain
 * `cd server`
 * `code .` to open VS Code
 * `make run-local` - builds server, then listens on port 8089, logs to stdout
 * (optional) `make test-local` to run server unit/integration tests without client

**Client**:

  * `code .` to open VS Code (at top level)
  * Click "reopen in container" button (to avoid installing Node.js and packages locally)
  * Open a terminal in VS Code (runs inside the dev container)
    - `cd client`
    - `npm install` to install dependencies
    - `npm run dev` to start dev HTTP on port 5173
  * Open http://localhost:5173/ in browser. Vite will reflect changes to the code
    while developing the Svete app, which is very handy. The client will connect Websocket to `ws://localhost:8089/` by default, so you can see what the server is doing in the other WSL terminal.

When done, at top level, run one of the following:

 * `make test` to build both client and server, and to run all tests in a pristine Docker container
 * `make debian-docker` to test and also build .debs (stored in `dist_deb/`)
 * `make run-docker` to automatically build .debs, install them in a Docker container, run server as a systemd service + client as a Nginx site inside it. This is the recommended way to test the whole stack
 * `make build-docker-demo` to build a demo image (like the one that can be found in Docker Hub)
