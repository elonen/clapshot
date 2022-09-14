from . import database

# TODO:
"""
# Support Ctrl+C
def signal_handler(signal, frame):
    global interrupt_flag
    if not interrupt_flag.is_set():
        logger.info("SIGINT received, exiting...")
        interrupt_flag.set()
signal.signal(signal.SIGINT, signal_handler)


...


        interrupt_flag = threading.Event()
        result_queue = queue.SimpleQueue()

        print(f"Starting monitor at {now()}...")
        p = threading.Thread(
            target=VideoProcessor.monitor_incoming_folder_loop,
            args=(vp, incoming_dir, dst_dir, interrupt_flag, result_queue, 0.1))
        p.start()
        time.sleep(1)
        
        print(f"Copying '{src}' to '{incoming_dir}' at {now()}...")
        shutil.copy(src, incoming_dir / src.name)
        shutil.copy(src_garbage, incoming_dir / src_garbage.name)
        time.sleep(1)
        
        print(f"Stopping monitor at {now()}...")
        interrupt_flag.set()
        print(f"Waiting for monitor & children to stop at {now()}...")
        p.join()

        # Check that both ok and corrupted files were processed
        res_ok = result_queue.get_nowait()



"""

