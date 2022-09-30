Clapshot server documentation
=============================

Clapshot server is backend part of the Clapshot video review web app,
that runs as a daemon on a Linux server (preferably Debian/Ubuntu).

You can easily try it out in Docker before installing. See `Installation`.

It requires Python 3.9+, SQLite libs 3.35+, FFMPEG and Nginx for hosting in production.

.. toctree::
   :maxdepth: 2
   :caption: Contents:

   introduction
   installation
   cli-commands
   architecture


.. rubric:: Modules

.. autosummary::
   :toctree: _autosummary
   :template: custom-module-template.rst
   :recursive:

   clapshot_server


Code documentation
==================

* :ref:`genindex`
* :ref:`modindex`
