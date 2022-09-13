#!/bin/bash
set -e

PYTHON=python3.9
REQ=requirements.txt
ACTIVATE=_venv/bin/activate

if [ ! -e _venv ]; then
  $PYTHON -m venv _venv
fi

source $ACTIVATE || { echo "Venv activation failed."; exit 1; }
pip install wheel 
pip install -r $REQ
python ./setup.py develop

echo " "
echo "---"
echo "Done. First run 'source $ACTIVATE'"
# echo "Then try 'lanscatter_master --help', 'lanscatter_peer' or 'lanscatter_gui'."