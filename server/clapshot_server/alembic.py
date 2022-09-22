# -*- coding: utf-8 -*-
import re
import sys
from pathlib import Path
from alembic.config import main as al_main, Config
import clapshot_server
import docopt

#from alembic.config import Config as AlembicCfg
#from alembic import command as alembic_cmd
#from alembic import script as alembic_script
#from alembic.runtime import migration

def main():
    """
    Database migration tool (Alembic wrapper) for Clapshot server.

    Passes all arguments to Alembic, but first configures Alembic to
    use the database in PATH.

    Usage:
      clapshot-alembic (--data-dir=PATH) [options] -- [<args>...]
      clapshot-alembic (-h | --help)

    Required:
     --data-dir=PATH      Directory for database, /incoming, /videos and /rejected

    Options:
     -h --help              Show this screen
     --showcmd              Print Alembic command, don't run it

    Examples:

      # Show current database revision
      clapshot-alembic --data-dir=DEV_DATADIR/ -- current

      # Show SQL for migrating to the latest revision (but don't execute it)
      clapshot-alembic --data-dir=/mnt/clapshot-data/data -- upgrade head --sql

      # Auto-upgrade database to latest revision:
      clapshot-alembic --data-dir=/mnt/clapshot-data/data -- upgrade head

      # Show Alembic help:
      clapshot-alembic --data-dir=/mnt/clapshot-data/data -- --help

      # Autogenerate new migrations (for Clapshot server developers):
      clapshot-alembic --data-dir=DEV_DATADIR/ -- revision --autogenerate -m "My new database changes"
    """
    args = docopt.docopt(main.__doc__)

    data_dir = Path(args["--data-dir"])
    if not (data_dir.exists() and data_dir.is_dir()):
        print(f"Data directory '{data_dir}' does not exist")
        return 1
    
    alb_cfg = Path(clapshot_server.__file__).parent / 'alembic.ini'
    if not alb_cfg.is_file():
        print(f"alembic.ini file '{alb_cfg}' does not exist")
        return 1

    mig_dir = alb_cfg.parent / 'sqlite-migrations'
    if not mig_dir.is_dir():
        print(f"alembic migrations directory '{mig_dir}' does not exist")
        return 1

    db_file = data_dir / "clapshot.sqlite"
    if not db_file.is_file():
        print(f"Database file '{db_file}' does not exist")
        return 1

    extra_opts = [
        '-c', str(alb_cfg),
        '-x', f'script_location={mig_dir.absolute()}',
        '-x', f'db_url=sqlite:///{db_file.absolute()}',
        ]


    sys.argv = sys.argv[:1] + extra_opts + args['<args>']
    sys.argv[0] = re.sub(r'(-script\.pyw|\.exe)?$', '', sys.argv[0])

    if args['--showcmd']:
        print(" ".join(sys.argv))
    else:
        sys.exit(al_main())

if __name__ == '__main__':
    main()
