#!/bin/bash
#
# Launch clapshot-server using settings from an INI config file.
#
# Each key in the [general] section becomes a command-line argument:
#   key = value   -> --key=value
#   key = true    -> --key            (boolean flag)
#   key = false   -> (omitted)
#
# Arguments are passed to the server directly, WITHOUT a second shell parse, so
# values may safely contain spaces, '*', commas, quotes and regex characters
# (e.g. notification-events = comment_*, media_file_added  works as expected).

CONF="${1:-}"
test -f "$CONF" || { echo "Error: Config file '$CONF' missing." >&2; exit 1; }

# Build the argument list, one element per emitted line, without word-splitting
# or globbing. (INI values are single-line, so newline separation is safe.)
ARGS=()
while IFS= read -r line; do
    ARGS+=("$line")
done < <(python3 - "$CONF" <<'EOF'
import configparser, sys
cfg = configparser.ConfigParser()
cfg.read(sys.argv[1])
for (k, v) in cfg.items('general'):
    # Strip one layer of matching surrounding quotes. Older versions launched the
    # server via `bash -c`, which stripped such quotes; do the same here so that
    # quoted values in existing config files keep working after an upgrade.
    if len(v) >= 2 and v[0] == v[-1] and v[0] in ('"', "'"):
        v = v[1:-1]
    if v.lower() == 'true':
        print(f'--{k}')
    elif v.lower() != 'false':
        print(f'--{k}={v}')
EOF
)

exec clapshot-server "${ARGS[@]}"
