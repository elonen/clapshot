#!/bin/bash

test -f "$1" || { echo "Error: Config file '$1' missing."; exit 1; }

# Combine lines from config file into cli arguments
ARGS=`python3 - <<EOF
import configparser
cfg = configparser.ConfigParser()
cfg.read("$1")
res=''
for (k,v) in cfg.items('general'):
  if v.lower() == 'true':
    res += f' --{k}'
  elif v.lower() != 'false':
    res += f' --{k}={v}'
print(res)
EOF`

exec bash -c "clapshot-server $ARGS"
