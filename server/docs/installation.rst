Installation
============

Trying it out
-------------

To try out Clapshot for the first time, do ``make run-docker``.
This builds everything in a Docker container, installs them in
a production-like environment, starts the server and an Nginx
proxy in front of it. Watch the console output for a URL to
open in a browser.


Deployment in production
------------------------

The server is started with the ``clapshot-server`` command, whch stays in the
foreground, and should therefore be started by a process manager.

Preferred deployment and upgrade method is to install it (and the client) as a Debian
package. It is a Python "omnibus" package, which means that it contains all
dependencies in a virtualenv. The package also contains a systemd service
file and config file ``/etc/`` that is translated into CLI options
on startup.

Server should be put behind a reverse proxy (e.g. nginx) in production, but
can be developed and tested without one. Debian package has an example
nginx config file in ``/usr/share/doc/clapshot-server/examples/``.


Building it
-----------

The recommended way to build it is to use Docker and the provided Makefile:

 #. Install and start Docker
 #. In the top level (not "server/""), run ``make debian-docker``

This will build Debian packages and put them in the ``dist_deb/`` directory.

You can also build the Debian packages directly on your system, but not recommended, as
it will leave behind a lot of build dependencies. See Makefiles and Dockerfiles for details.


Running tests
-------------

Run ``make test`` to run the tests in a Docker container.

During development running unit tests on clapshot-server can be more
convenient to do locally (inside a Python _venv).

Install a _venv (``cd server && make dev``) and run ``pytest``.
