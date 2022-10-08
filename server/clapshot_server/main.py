'''
Entry point for the Clapshot server.

This is a command line program that stays in the foreground,
so background it with process control tools like systemd or supervisord.
'''

import asyncio
import logging
import multiprocessing
from pathlib import Path
import docopt
from clapshot_server import database, api_server, video_processing_pipeline, video_ingesting, multi_processor


def main():
    """
    Clapshot server - backend of a video annotation tool

    Monitors <path>/incoming for new videos, processes them, and stores them in <path>/videos.
    Then serves the annotations and comments via an asyncronous HTTP + Socket.IO API.
    Use a proxy server to serve files in /videos and to secure the API with HTTPS/WSS.

    Usage:
      clapshot-server [options] (--url-base=URL) (--data-dir=PATH)
      clapshot-server [options] [--mute TOPIC]... (--url-base=URL) (--data-dir=PATH)
      clapshot-server (-h | --help)

    Required:
     --url-base=URL       Base URL of the API server, e.g. https://example.com/clapshot/.
                          This depends on your proxy server configuration.
     --data-dir=PATH      Directory for database, /incoming, /videos and /rejected

    Options:
     -p PORT --port=PORT    Port to listen on [default: 8095]
     -H HOST --host=HOST    Host to listen on [default: 0.0.0.0]
     --host-videos          Host the /videos directory [default: False]
                            (For debugging. Use Nginx or Apache with auth in production.)
     -P SEC --poll SEC      Polling interval for incoming folder [default: 3.0]
     -m TOPIC --mute TOPIC    Mute logging for a topic (can be repeated). Sets level to WARNING.
                            See logs logs for available topics.
     -l FILE --log FILE     Log to file instead of stdout
     -w N --workers N       Max number of workers for video processing [default: 0]
                            (0 = number of CPU cores)
     -d --debug             Enable debug logging
     -h --help              Show this screen
    """
    args = docopt.docopt(main.__doc__)

    logging.basicConfig(
        level = (logging.DEBUG if args["--debug"] else logging.INFO),
        format='%(asctime)s %(name)-12s %(levelname)-8s %(message)s',
        datefmt='%m-%d %H:%M:%S',
        filename=args["--log"] or None
    )
    logger = logging.getLogger("main")


    # Mute logging for some topics
    for topic in args["--mute"] or []:
        logging.getLogger(topic).setLevel(logging.WARNING)

    # Make sure data dir exists
    data_dir = Path(args["--data-dir"])
    if not (data_dir.exists() and data_dir.is_dir()):
        logger.error(f"Data directory '{data_dir}' does not exist")
        return 1

    incoming_dir = data_dir / "incoming"
    videos_dir = data_dir / "videos"
    upload_dir = data_dir / "upload"
    rejected_dir = data_dir / "rejected"
    for d in (incoming_dir, videos_dir, rejected_dir, upload_dir):
        d.mkdir(exist_ok=True)

    url_base = args["--url-base"]
    assert url_base

    db_file = data_dir / "clapshot.sqlite"

    async def go(mpm):

        push_message_queue = asyncio.Queue()

        vip = video_processing_pipeline.VideoProcessingPipeline(
            db_file = db_file,
            data_dir = data_dir,
            mpm = mpm,
            max_workers = int(args["--workers"]))

        vip_proc = multiprocessing.Process(
            target=vip.run_forever,
            args=[float(args["--poll"])])
        vip_proc.start()

        # Run API server with asyncio forever
        async def run_api_server() -> bool:
            return await api_server.run_server(
                db=database.Database(
                    db_file,
                    logging.getLogger("db")),
                logger=logging.getLogger("api"),
                url_base=url_base,
                videos_dir=videos_dir,
                upload_dir=upload_dir,
                host=args["--host"],
                port=int(args["--port"]),
                push_messages=push_message_queue,
                serve_dirs={'/video': videos_dir} if args["--host-videos"] else {},
                ingest_callback=lambda fn, user_id: vip.queue_for_ingestion(fn, user_id))

        async def vp_result_deliverer():
            while vip_proc.is_alive():
                await asyncio.sleep(0.5)
                if not vip.res_to_user.empty():
                    vp_res = vip.res_to_user.get()  # type: video_ingesting.UserResults
                    await push_message_queue.put(database.Message(
                        event_name = ('ok' if vp_res.success else 'error'),
                        user_id = vp_res.file_owner_id,
                        ref_video_hash = vp_res.video_hash,
                        message = vp_res.msg or '',
                        details = vp_res.details or ''))
        
        try:
            task_api = asyncio.create_task(run_api_server())
            task_msg = asyncio.create_task(vp_result_deliverer())
            while vip_proc.is_alive() and \
                  not task_api.done() and \
                  not task_msg.done():
                await asyncio.sleep(0.2)
        except KeyboardInterrupt:
            pass
        finally:
            vip_proc.terminate()

        logger.info("API server stopped")

    try:
        multi_processor.install_sigterm_handlers()
        with multiprocessing.Manager() as mpm:
            asyncio.run(go(mpm))
    except KeyboardInterrupt:
        pass
    finally:
        logger.info("Bye")

if __name__ == '__main__':
    main()
