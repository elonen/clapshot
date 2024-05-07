#!/bin/bash
DIR=$(dirname "$0")
$DIR/_venv/bin/python -m organizer.main $@
