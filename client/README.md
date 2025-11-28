# Clapshot client
## Frontend components for Clapshot video review tool

This is the web frontend component for Clapshot - a self-hosted collaborative
video review tool. See github page for an overview of the whole tool.

It's a Svelte app that is built into a static site, and served by
Nginx or some other web server. When loaded, it first fetches
`clapshot_client.conf.json`, reads the server URL from it, and then
attempts to connect to the server via websocket.

If you have installed it in a Debian system, the config file is
symlinked to `/etc/clapshot_client.conf`. Otherwise it's located
in the same directory as the `index.html` file.

## Localization

The UI supports multiple locales (English and Simplified Chinese included).
Set `default_locale`/`supported_locales` in `clapshot_client.conf.json`, then
switch languages from the user menu in the header.
