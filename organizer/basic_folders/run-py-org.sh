#!/bin/bash
DIR=$(dirname "$0")
cd "$DIR"
exec _venv/bin/python -m organizer.main $@
