Command line interface
======================

Although .deb packages and the bundled config file are the recommended way to
run Clapshot in production, it is also possible to run the server directly
from the command line and have all logging go to stdout.
This is useful for development and debugging.

The ``clapshot-server`` command starts server. It is implemented in the
`clapshot_server.main` module.

.. literalinclude:: ../clapshot_server/main.py
   :language: none
   :start-after: """
   :end-before: """

Upgrading database
------------------

The ``clapshot-alembic`` command is used to upgrade the database
schema when the server is upgraded to a version that is not compatible
with the current DB. It's a wrapper for the vanilla ``alembic`` tool.

Server refuses to start if the database schema is not up-to-date, so
you can't really forget to do this by accident.

.. literalinclude:: ../clapshot_server/alembic.py
   :language: none
   :start-after: """
   :end-before: """

