#!/bin/bash
DIR=$(dirname "$0")
#export PYTHONPATH="$PYTHONPATH;$DIR"
cd "$DIR"
_venv/bin/python -m organizer.main $@
