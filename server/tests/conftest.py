import pytest
from pytest_alembic.config import Config as PTAConfig
import sqlalchemy

import clapshot_server
from pathlib import Path


@pytest.fixture
def alembic_config():
    """Override this fixture to configure the exact alembic context setup required.
    """
    #script = Path(clapshot_server.__file__).parent / "sqlite-migrations"
    ini = Path(clapshot_server.__file__).parent / "alembic.ini"
    return PTAConfig({
        'script_location': 'clapshot_server/sqlite-migrations',
        'file': str(ini)
    })

#@pytest.fixture
#def alembic_engine():
#    """Override this fixture to provide pytest-alembic powered tests with a database handle.
#    """
#    return sqlalchemy.create_engine("sqlite:///")
