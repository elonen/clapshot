#from logging.config import fileConfig

from sqlalchemy import create_engine
import sys

from alembic import context
from alembic.runtime import migration

# this is the Alembic Config object, which provides
# access to the values within the .ini file in use.
config = context.config

# Interpret the config file for Python logging.
# This line sets up loggers basically.

#if config.config_file_name is not None:
#    fileConfig(config.config_file_name, disable_existing_loggers=False)


# Enable autogenerate

#target_metadata = None
from clapshot_server.database import Base
target_metadata = Base.metadata

# other values from the config, defined by the needs of env.py,
# can be acquired:
# my_important_option = config.get_main_option("my_important_option")
# ... etc.


def _omit_sqlite_sequence(object, name, type_, reflected, compare_to):
    return name not in ["sqlite_sequence"]

def _get_db_url():
    url = context.get_x_argument(as_dictionary=True).get('db_url')
    assert url, "URL not set in Alembic args. (e.g. -x db_url=sqlite:///path/to/db.sqlite)"
    return url


def run_migrations_offline() -> None:
    """Run migrations in 'offline' mode.

    This configures the context with just a URL
    and not an Engine, though an Engine is acceptable
    here as well.  By skipping the Engine creation
    we don't even need a DBAPI to be available.

    Calls to context.execute() here emit the given string to the
    script output.

    """
    context.configure(
        url=_get_db_url(),
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
        include_object=_omit_sqlite_sequence,
        compare_type=True
    )

    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    """Run migrations in 'online' mode.

    In this scenario we need to create an Engine
    and associate a connection with the context.

    """
    connectable = context.config.attributes.get("connection", None)

    if connectable is None:
        connectable = create_engine(_get_db_url())

    with connectable.connect() as connection:
        context.configure(
            connection=connection, target_metadata=target_metadata,
            include_object=_omit_sqlite_sequence
        )

        mc = migration.MigrationContext.configure(connection)
        print("Head(s) before: " + ' '.join(mc.get_current_heads()))
        print("Running online migrations...")

        with context.begin_transaction():
            context.run_migrations()
        
        print("Head(s) now: " + ' '.join(mc.get_current_heads()))


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
