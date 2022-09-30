Introduction
------------

The server has three main functions:

  1. **API server**, which listens to Socket.IO (websocket or HTTP long polling) connections
     from the web frontend and/or pushes messages when something happens.
  2. **Video file processing**, which receives files, extracts metadata and transcodes if necessary.
  3. **Database**, which stores video metadata, user comments etc.

These are independent components and run in separate OS processes. Video processing forks into several
subprocessing pools.

They sub-processes currently communicate through internal multiprocessing queues, but those
could be quite painlessly be replaced with something else (e.g. ZeroMQ, Redis or RabbitMQ) if
distribution to multiple machines becomes necessary.

The database is an SQLite3 file, which is stored in ``data-dir`` (see CLI arguments).
Database migrations - when schema changes during version upgrades - can be done with the Alembic
tool, that is wrapped (for convenience) as a separate command-line tool, ``clapshot-alembic``.

Data directory should be put on a large file system. It contains:

 * ``incoming/``  - For incoming files (files can submitted by Samba or NFS).
 * ``videos/``    - Directory where video files are stored after processing (in subdirectories).
 * ``rejected/``  - Directory where rejected files are moved to if processing fails. (This should also be shared to user via Samba/NFS.)


Technology overview
-------------------

 * Python 3.9+ (with asyncio and multiprocessing)
 * Pytest for testing
 * SQLAlchemy for database access
 * Socket.IO for communicating with client
 * FFmpeg for video transcoding
 * MediaInfo for video metadata extraction
 * Alembic (with custom wrapper) for database migrations
 * Sphinx for generating docs
 * Docopt for CLI argument parsing
 * Docker (optional) for tests and building
 * Debian GNU/Linux for hosting (.deb packages are preferred deployment method). Clapshot server probably won't work properly on Windows or MacOS.
 * Samba and NFS for submitting files (optional)
 * Nginx (in production), for

   * authentication (Kerberos, HTTP auth, or anything)
   * serving videos
   * serving web client (HTML, JS, CSS)
   * encrypting HTTP and Websocket traffic
   * proxying to API server (which should only be exposed to localhost)
