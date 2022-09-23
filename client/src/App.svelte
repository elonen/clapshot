<script lang="ts">
  import {io, Socket} from 'socket.io-client'

  import { fade, slide, scale } from "svelte/transition";

  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import VideoPlayer from './lib/VideoPlayer.svelte';
  import CommentInput from './lib/CommentInput.svelte';
  import UserMessage from './lib/UserMessage.svelte';
  import {Notifications, acts} from '@tadashi/svelte-notification'
  
  import {all_comments, cur_username, cur_user_id, video_is_ready, video_url, video_hash, video_fps, video_orig_filename, all_my_videos, user_messages} from './stores.js';
    
  let video_player: VideoPlayer;
  let comment_input: CommentInput;

  // Messages from CommentInput component
  function onCommentInputButton(e) {
    console.log("Comment Input Button Clicked: " + e.detail.action);
    if (e.detail.action == "send")
    {
      if (e.detail.comment_text != "")
      {
        socket.emit('add_comment', {
          video_hash: $video_hash,
          parent_id: null,            // TODO: parent id here
          comment: e.detail.comment_text,
          drawing: video_player.getDrawing(),
          timecode: e.detail.is_timed ? video_player.getCurTimecode() : "",
        });
      }
    }
    else if (e.detail.action == "color_select") {
      video_player.onColorSelect(e.detail.color);
    }
    else if (e.detail.action == "draw") {
      video_player.onToggleDraw(e.detail.is_draw_mode);
    }
    else if (e.detail.action == "undo") {
      video_player.onDrawUndo();
    }
    else if (e.detail.action == "redo") {
      video_player.onDrawRedo();
    }
  }


  function onDisplayComment(e) {
    video_player.seekTo(e.detail.timecode, 'SMPTE');
    // Close draw mode while showing (drawing from a saved) comment
    video_player.onToggleDraw(false);
    comment_input.forceDrawMode(false);
    if (e.detail.drawing)    
      video_player.setDrawing(e.detail.drawing);
  }
  function onDeleteComment(e) {
    socket.emit('del_comment', {
      comment_id: e.detail.comment_id,
    });
  }
  function onReplyComment(e) {    
    socket.emit('add_comment', {
        video_hash: $video_hash,
        parent_id: e.detail.parent_id,
        comment: e.detail.comment_text,
      });
  }

  function onEditComment(e) {
    socket.emit('edit_comment', {
      comment_id: e.detail.comment_id,
      comment: e.detail.comment_text,
    });
  }

  function onSeekToTimecode(e) {
    console.log("Seek to timecode: " + e.detail.timecode);
    video_player.seekTo(e.detail.timecode, 'SMPTE');
  }


  function closeVideo() {
    // Close current video, list all user's own videos.
    // This is called from onClearAll event and history.back()
    console.log("Clear all");
    $video_hash = null;
    $video_url = null;
    $video_fps = null;
    $video_orig_filename = null;
    $all_comments = [];
    $video_is_ready = false;
    socket.emit('list_my_videos', {});
    socket.emit('list_my_messages', {});
  }

  function onClearAll(e) {
    history.pushState('/', null, '/');  // Clear URL
    closeVideo();
  }

  function onClickVideo(new_video_hash) {
    socket.emit('open_video', {video_hash: new_video_hash});
    history.pushState(new_video_hash, null, '/?vid='+new_video_hash);  // Point URL to video
  }


  function popHistoryState(e) {
    console.log("popHistoryState: " + e.state);
    if (e.state && e.state !== '/')
      socket.emit('open_video', {video_hash: e.state});
    else
      closeVideo();
  }


  // Parse URL to see if we have a video to open
  const urlParams = new URLSearchParams(window.location.search);
  urlParams.forEach((value, key) => {
    if (key != "vid") {
      console.log("Got UNKNOWN URL parameter: '" + key + "'. Value= " + value);
      acts.add({mode: 'warn', message: "Unknown URL parameter: '" + key + "'", lifetime: 5});
    }
  });
  $video_hash = urlParams.get('vid');
  if ($video_hash) {
    console.log("Video hash: " + video_hash);
    history.pushState($video_hash, null, '/?vid='+$video_hash);
  }


  let socket: Socket;


  // -------------------------------------------------------------
  // Socket.io messaging
  // -------------------------------------------------------------

  // Read config from HTTP server first
  const conf_file = "clapshot_client.conf.json";
  function handleErrors(response: any) {
    if (!response.ok)
        throw Error("HTTP error: " + response.status);
    return response;
  }
  fetch(conf_file)
      .then(handleErrors)
      .then(response => response.json())
      .then(json => connect_socket_io(json.api_url))
      .catch(error => {
          console.log("Failed to read config. " + error)
          acts.add({mode: 'danger', message: "Failed to read config. " + error, lifetime: 50});
        });


  // This is called after we get the API URL from the server  
   function connect_socket_io(api_url: string)
  {
    if (!api_url)
      throw Error("API URL not specified in config file");
    
    console.log("...CONNECTING to API: " + api_url);
    socket = io(api_url, {
      path: '/api/socket.io',
      extraHeaders: {x_remote_user_id: "anonymous", x_remote_user_name: "Anonymous (no auth)"},
      timeout: 5000, // 5 seconds
    })

    socket.on('connect', () => {
      console.log("Socket connected");
      //acts.add({mode: 'info', message: 'Connected.', lifetime: 1.5});
      if ($video_hash) {
        socket.emit('open_video', {video_hash: $video_hash});
      } else {
        socket.emit('list_my_videos', {});
        socket.emit('list_my_messages', {});
      }
    });


    function handle_with_errors(func) {
      // Workaround wrapper to show errors from socket.io callbacks
      // (socket.io swallows exceptions and reconnects silently on errors)
      try {
        return func();
      } catch (e) {
        // log message, fileName, lineNumber
        console.log("Exception in Socket.IO handler: ", e);
        console.log(e.stack);
        acts.add({mode: 'danger', message: 'Client error: ' + e, lifetime: 5});
      }
    }

    socket.on('connect_failed', (data) => handle_with_errors(() => {
      console.log("Socket connect failed");
      acts.add({mode: 'danger', message: 'Connection failed.', lifetime: 5});
    document.write("API connection failed.");
    acts.add({mode: 'warn', message: 'API connection failed.', lifetime: 1.5});
    }));

    socket.on('welcome', (data) => handle_with_errors(() => {
      console.log("[SERVER] welcome: " + JSON.stringify(data));
      $cur_username = data.username;
      $cur_user_id = data.user_id
    }));

    socket.on('message', (data) => handle_with_errors(() => {
      console.log("[SERVER] message: " + JSON.stringify(data));
      $user_messages = $user_messages.filter((m) => m.id != data.id);
      $user_messages.push(data);
      $user_messages = $user_messages.sort((a, b) => a.id > b.id ? -1 : a.id < b.id ? 1 : 0)

      if (!data.seen) {
        const severity = (data.event_name == 'error') ? 'danger' : 'info';
        acts.add({mode: severity, message: data.message, lifetime: 5});
      }
    }));

    socket.on('error', (data) => handle_with_errors(() => {
      console.log("[SERVER ERROR]: " + JSON.stringify(data));
      acts.add({mode: 'danger', message: data.msg, lifetime: 5});
    }));


    socket.on('user_videos', (data) => handle_with_errors(() => {
      $all_my_videos = data.videos;
    }));

    socket.on('open_video', (data) => handle_with_errors(() => {
      console.log("[SERVER] open_video: " + JSON.stringify(data));
      $video_url = data.video_url;
      $video_hash = data.video_hash;
      $video_fps = data.fps;
      $video_orig_filename = data.orig_filename;    
      $all_comments = [];
    }));



    socket.on('new_comment', (data) => handle_with_errors(() => 
    {
      function reorder_comments(old_order) {
        // Helper to show comment threads in the right order and with correct indentation
        let old_sorted = old_order.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
        let new_order = [];
        function find_insert_position_and_indent(parent_id)
        {
          if (parent_id) {
            for (let i=new_order.length-1; i>=0; i--) {
              if (new_order[i].id == parent_id)
                return [i, new_order[i].indent+1] as const;
              if (new_order[i].parent_id == parent_id)
                return [i, new_order[i].indent] as const;
            }}
          return [new_order.length-1, 0] as const;
        }
        old_sorted.forEach((comment) => {
          let [pos, indent] = find_insert_position_and_indent(comment.parent_id);
          new_order.splice(pos+1, 0, {...comment, indent: indent});
        });
        return new_order;
      }

      console.log("[SERVER] new_comment id=" + data.comment_id + " parent_id=" + data.parent_id + " tc=" + data.timecode + " comment=" + data.comment);
      if (data.video_hash == $video_hash)
      {
        $all_comments.push({
            id: data.comment_id,
            comment: data.comment,
            username: data.username,
            user_id: data.user_id,
            avatar_url: null,
            drawing_data: data.drawing,
            parent_id: data.parent_id,
            edited: data.edited,
            indent: 0,
            timecode: data.timecode
          });
        $all_comments = reorder_comments($all_comments);
      } else {
        console.log("Comment not for this video. Ignoring.");
      }
    }));

    socket.on('del_comment', (data) => handle_with_errors(() => {
      console.log("[SERVER] del_comment: " + data.comment_id);
      $all_comments = $all_comments.filter((c) => c.id != data.comment_id);
    }));

    //socket.onAny((eventName, data) => {
    //  console.log("[SERVER] TYPE '"+eventName+"': " + JSON.stringify(data));
    //});
  }

</script>

<svelte:window on:popstate={popHistoryState}/>
<main>
  <div class="w-full mb-4"><NavBar on:clear-all={onClearAll} /></div>
  <div class="flex">
    <div class="flex-1 bg-black">
      <div class="grid" style="width: 100%;">
        <Notifications />

        {#if !(socket && socket.connected) }

          <!-- ========== "connecting" spinner ============= -->
          <div transition:scale class="w-full h-full text-5xl text-slate-600 align-middle text-center">
            <h1 class="m-16" style="font-family: 'Yanone Kaffeesatz', sans-serif;">
              Connecting server...
            </h1>
            <div class="fa-2x block">
              <i class="fas fa-spinner connecting-spinner"></i>
            </div>            

          </div>

        {:else if $video_hash}

          <!-- ========== video review widgets ============= -->
          <div transition:slide class="flex w-full">

            <div class="flex-0 transition:slide">
              <div class="block bg-cyan-900">
                <VideoPlayer bind:this={video_player} src={$video_url} />
              </div>
              <div class="block w-full p-4">
                <CommentInput bind:this={comment_input} on:button-clicked={onCommentInputButton} />
              </div>      
            </div>

            {#if $all_comments.length > 0}
            <!-- ========== comment sidepanel ============= -->
            <div class="flex-1">
              <div class="flex-none basis-128 bg-gray-900 w-80 py-2 px-2 space-y-2 ml-2 h-screen overflow-y-scroll" transition:slide>
                {#each $all_comments as item}
                  <CommentCard {...item} on:display-comment={onDisplayComment} on:delete-comment={onDeleteComment} on:reply-to-comment={onReplyComment} on:edit-comment={onEditComment}/>
                {/each}
              </div>
            </div>
            {/if}
          </div>


        {:else}

          <!-- ========== video listing ============= -->        
          <div transition:slide class="m-6 text">            
            <h1 class="text-4xl m-6">
              {#if $all_my_videos.length>0}
                All your videos
              {:else}
                You have no videos.
              {/if}
            </h1>
            <div class="gap-8">
              {#each $all_my_videos as item}
              <div class="bg-slate-600 rounded-md p-2 m-1 mx-6 inline-block cursor-pointer" on:click|preventDefault="{()=>onClickVideo(item.video_hash)}">
                <span class="text-amber-400 text-xs pr-2 border-r border-gray-400">{item.added_time}</span>
                <span class="text-amber-500 font-mono text-xs pr-2 border-r border-gray-400">{item.video_hash}</span>
                <a href="/?vid={item.video_hash}" class="text-xs overflow-clip whitespace-nowrap">{item.orig_filename}</a>
              </div>          
              {/each} 
            </div> 

            {#if $user_messages.length>0}
              <h1 class="text-2xl m-6 mt-12 text-slate-500">
                  Latest messages
              </h1>
              <div class="gap-4">
                {#each $user_messages as msg}
                  <UserMessage {msg} />
                {/each} 
              </div> 
            {/if}


          </div>

        {/if}
      </div>
    </div>
    
  </div>
</main>

<style>
/* Animate "waiting for server" spinner */
.connecting-spinner { animation: rotation 3s infinite steps(8); }
@keyframes rotation { 
    from { 
        transform: rotate(0deg); 
    } to { 
        transform: rotate(360deg); 
    }
}</style>

