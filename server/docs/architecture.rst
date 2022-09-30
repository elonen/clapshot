Architecture
============

The server comprises of a number of separate processes that pass messages to each other through
multiprocessing safe queues.

Here's an overview on what happens when a client copies a new video file to the "incoming" directory:

.. mscgen::

   msc {
      width = "1024";

      Client, API [label="API server"], Mon [label="Incoming monitor"], Pipeline [label="Processing pipeline"], Meta [label="Metadata reader(xN)"], Ing [label="Video ingesting(xN)"], Comp [label="Video compressor(xN)"];

      Client note Client [label="Copy video to 'incoming/' folder"];

      Mon note Mon [label="Finds new file"];

      Mon => Pipeline [label="file added"];
      Pipeline => Meta [label="MD read request"];
      Meta => Pipeline [label="Video metadata"];
      Pipeline => Ing [label="Path and metadata"];

      Ing note Ing [label="Move file to 'videos/[HASH]/'"];
      Ing note Ing [label="Add to DB"];

      Ing => Comp [label="src filename"];
      Ing => Pipeline [label="MSG: 'Video added'"];

      API << Pipeline [label="(poll user messages)"];
      API => Client [label="MSG: 'Video added'"];
      ...;
      Comp => Pipeline [label="transcoding results"];
      Pipeline => Ing [label="transcoding results"];

      Ing note Ing [label="Symlink transcoded file"];
      Ing note Ing [label="Mark recompressed in DB"];
      Ing => Pipeline [label="MSG: 'Video transcoded'"];

      API << Pipeline [label="(poll user messages)"];
      API => Client [label="MSG: 'Video transcoded'"];
   }

Components marked with "(xN)" are a pool several workers, that all listen to the same task queue.

`Processing pipeline` is mostly just a "paper pusher" that owns all the queues and reduces
codependecies between the other components.

If errors occur during the processing, the messages contain "success = False",
along with an error message, and possible details.
