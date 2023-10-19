<script lang="ts">
  import {fade, slide} from "svelte/transition";
  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import VideoPlayer from './lib/VideoPlayer.svelte';
  import CommentInput from './lib/CommentInput.svelte';
  import UserMessage from './lib/UserMessage.svelte';
  import FileUpload from './lib/FileUpload.svelte';
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import VideoListPopup from './lib/VideoListPopup.svelte';

  import {all_comments, cur_username, cur_user_id, video_is_ready, video_url, video_hash, video_fps, video_title, all_my_videos, user_messages, video_progress_msg, collab_id, user_menu_items} from './stores.js';

  let video_player: VideoPlayer;
  let comment_input: CommentInput;
  let debug_layout: boolean = false;
  let ui_connected_state: boolean = false; // true if UI should look like we're connected to the server

  let last_video_progress_msg_ts = Date.now();  // used to hide video_progress_msg after a few seconds

  let collab_dialog_ack = false;  // true if user has clicked "OK" on the collab dialog
  let last_collab_controlling_user = null;    // last user to control the video in a collab session

  function log_abbreviated(str: string) {
      const max_len = 180;
      if (str.length > max_len)
        str = str.substr(0, max_len) + "(...)";
      console.log(str);
  }

  // Messages from CommentInput component
  function onCommentInputButton(e) {
    if (e.detail.action == "send")
    {
      if (e.detail.comment_text != "")
      {
        ws_emit('add_comment', {
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
    if ($collab_id) {
      console.log("Collab: onDisplayComment. collab_id: '" + $collab_id + "'");
      ws_emit('collab_report', {paused: true, seek_time: video_player.getCurTime(), drawing: e.detail.drawing});
    }
  }

  function onDeleteComment(e) {
    ws_emit('del_comment', {
      comment_id: e.detail.comment_id,
    });
  }

  function onReplyComment(e) {
    ws_emit('add_comment', {
        video_hash: $video_hash,
        parent_id: e.detail.parent_id,
        comment: e.detail.comment_text,
      });
  }

  function onEditComment(e) {
    ws_emit('edit_comment', {
      comment_id: e.detail.comment_id,
      comment: e.detail.comment_text,
    });
  }

  function onSeekToTimecode(e) {
    video_player.seekTo(e.detail.timecode, 'SMPTE');
  }


  function closeVideo() {
    // Close current video, list all user's own videos.
    // This is called from onClearAll event and history.back()
    console.log("closeVideo");
    ws_emit('leave_collab', {});
    $collab_id = null;
    $video_hash = null;
    $video_url = null;
    $video_fps = null;
    $video_title = null;
    $all_comments = [];
    $video_is_ready = false;
    ws_emit('list_my_videos', {});
    ws_emit('list_my_messages', {});
  }

  function onClearAll(e) {
    history.pushState('/', null, '/');  // Clear URL
    closeVideo();
  }

  function onClickVideo(new_video_hash) {
    ws_emit('open_video', {video_hash: new_video_hash});
    history.pushState(new_video_hash, null, '/?vid='+new_video_hash);  // Point URL to video
  }

  function onVideoSeeked(e) {
    //console.log("App: seeked()");
    comment_input.forceDrawMode(false);  // Close draw mode when video frame is changed
  }

  function onCollabReport(e) {
    if ($collab_id)
      ws_emit('collab_report', {paused: e.detail.paused, seek_time: e.detail.seek_time, drawing: e.detail.drawing});
  }

  function onCommentPinClicked(e) {
      // Find corresponding comment in the list, scroll to it and highlight
      let comment_id = e.detail.id;
    let comment = $all_comments.find(c => c.id == comment_id);
    if (comment) {
      onDisplayComment({detail: {timecode: comment.timecode, drawing: comment.drawing_data}});
      let comment_card = document.getElementById("comment_card_" + comment_id);
      if (comment_card) {
        comment_card.scrollIntoView({behavior: "smooth", block: "center", inline: "nearest"});
        setTimeout(() => { comment_card.classList.add("highlighted_comment"); }, 500);
        setTimeout(() => { comment_card.classList.remove("highlighted_comment"); }, 3000);
      }
    }
  }

  function popHistoryState(e) {
    console.log("popHistoryState: " + e.state);
    if (e.state && e.state !== '/')
      ws_emit('open_video', {video_hash: e.state});
    else
      closeVideo();
  }

  // Parse URL to see if we have a video to open
  const urlParams = new URLSearchParams(window.location.search);
  urlParams.forEach((value, key) => {
    if (key != "vid" && key != "collab") {
      console.log("Got UNKNOWN URL parameter: '" + key + "'. Value= " + value);
      acts.add({mode: 'warn', message: "Unknown URL parameter: '" + key + "'", lifetime: 5});
    }
  });
  $video_hash = urlParams.get('vid');
  const prev_collab_id = $collab_id;
  $collab_id = urlParams.get('collab');
  if ($video_hash) {
    // console.log("Video hash: " + video_hash);
    if ($collab_id)
      history.pushState($video_hash, null, '/?vid='+$video_hash+'&collab='+$collab_id);
    else
      history.pushState($video_hash, null, '/?vid='+$video_hash);
  }

  let upload_url: string = "";

  // -------------------------------------------------------------
  // Websocket messaging
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
      .then(json => {
        // Check that we have all the expected config lines
        const expected = ["ws_url", "upload_url", "user_menu_extra_items", "user_menu_show_basic_auth_logout"];
        for (let key of expected) {
          if (!(key in json))
            throw Error("Missing key '" + key + "' in client config file '" + conf_file + "'");
        }

        upload_url = json.upload_url;
        connect_websocket(json.ws_url);

        $user_menu_items = json.user_menu_extra_items;
        if (json.user_menu_show_basic_auth_logout) {
          $user_menu_items = [...$user_menu_items, {label: "Logout", type: "logout-basic-auth"}];
        }
      })
      .catch(error => {
          console.log("Failed to read config. " + error)
          acts.add({mode: 'danger', message: "Failed to read config. " + error, lifetime: 50});
        });


  let video_list_refresh_scheduled = false;
  function refresh_my_videos()
  {
    if (!video_list_refresh_scheduled) {
      video_list_refresh_scheduled = true;
      setTimeout(() => {
        video_list_refresh_scheduled = false;
        ws_emit('list_my_videos', {});
      }, 500);
    }
  }



  let ws_socket: WebSocket;

  function is_connected() {
    return ws_socket && ws_socket.readyState == ws_socket.OPEN;
  }

  function disconnect() {
    closeVideo();
    if (ws_socket) {
      ws_socket.close();
    }
    ui_connected_state = false;
  }


  let send_queue: any[] = [];

  // Send message to server. If not connected, queue it.
  function ws_emit(event_name: string, data: any)
  {
    let raw_msg = JSON.stringify({cmd: event_name, data: data});
    if (is_connected()) {
      log_abbreviated("ws_emit(): Sending: " + raw_msg);
      ws_socket.send(raw_msg);
    }
    else {
      console.log("ws_emit(): Disconnected, so queuing: " + raw_msg);
      send_queue.push(raw_msg);
    }
  }

  // Infinite loop that sends messages from the queue.
  // This only ever sends anything if ws_emit() queues messages due to temporary disconnection.
  function send_queue_loop()
  {
    while (send_queue.length > 0) {
      let raw_msg = send_queue.shift();
      ws_socket.send(raw_msg);
    }
    setTimeout(send_queue_loop, 500);
  }
  setTimeout(send_queue_loop, 500); // Start the loop


  let reconnect_delay = 100;  // for exponential backoff


  function connect_websocket(ws_url: string) {
    const auth_url = ws_url.replace(/^wss:/, "https:").replace(/^ws:/, "http:").replace(/\/api\/.*$/, "/api/health");

    function schedule_reconnect() {
        reconnect_delay = Math.round(Math.min(reconnect_delay * 1.5, 5000));
        console.log("API reconnecting in " + reconnect_delay + " ms");
        setTimeout(() => { connect_websocket(ws_url); }, reconnect_delay);
        setTimeout(() => { if (!is_connected()) ui_connected_state = false; }, 3000);
    }

    try {
        return fetch(auth_url)
          .then(response => {
            if (response.ok) {
                console.log("Authentication check OK. Connecting to WS API");
                return connect_websocket_after_auth_check(ws_url);
            } else if (response.status === 401 || response.status === 403) {
                console.log("Auth failed. Status: " + response.status);
                if (reconnect_delay > 1500) {
                  // Force full reload to show login page
                  window.location.reload();
                }
            } else {
                throw new Error(`HTTP auth check ERROR: ${response.status}`);
            }
            schedule_reconnect();
          })
          .catch(error => {
            console.error('HTTP auth check failed:', error);
            schedule_reconnect();
          });
      } catch (error) {
        schedule_reconnect();
      }
  }


  // Called after we get the API URL from the server.
  function connect_websocket_after_auth_check(ws_url: string)
  {
    if (!ws_url)
      throw Error("API URL not specified in config file");

    console.log("...CONNECTING to WS API: " + ws_url);
    ws_socket = new WebSocket(ws_url);


    // Handle connection opening
    ws_socket.addEventListener("open", function (event) {
      reconnect_delay = 100;
      ui_connected_state = true;

      console.log("Socket connected");
      //acts.add({mode: 'info', message: 'Connected.', lifetime: 1.5});
      if ($video_hash) {
        ws_emit('open_video', {video_hash: $video_hash});
      } else {
        ws_emit('list_my_videos', {});
        ws_emit('list_my_messages', {});
      }
    });

    function handle_with_errors(func) {
      try {
        return func();
      } catch (e) {
        // log message, fileName, lineNumber
        console.log("Exception in Websocket handler: ", e);
        console.log(e.stack);
        acts.add({mode: 'danger', message: 'Client error: ' + e, lifetime: 5});
      }
    }

    /*
    ws_socket.addEventListener("error", function (event) {
      handle_with_errors(() => {
        console.log("Websocket error: " + event);
      });
    });
    */

    // Reconnect if closed, with exponential+random backoff
    ws_socket.addEventListener("close", function (event) {
      console.log("Websock kocket closed:", event);

      reconnect_delay = Math.round(Math.min(reconnect_delay * 1.5, 5000));
      console.log("API reconnecting in " + reconnect_delay + " ms");
      setTimeout(() => { connect_websocket(ws_url); }, reconnect_delay);
      setTimeout(() => { if (!is_connected()) ui_connected_state = false; }, 3000);
    });

    if (prev_collab_id != $collab_id) {
      // We have a new collab id. Close old and open new one.
      if (prev_collab_id)
        ws_emit('leave_collab', {});
      if ($collab_id)
        ws_emit('join_collab', {collab_id: $collab_id, video_hash: $video_hash});
    }

    // Incoming messages
    ws_socket.addEventListener("message", function (event)
    {
      const msg_json = JSON.parse(event.data);
      handle_with_errors(() =>
      {
        const cmd = msg_json.cmd;
        const data = msg_json.data;

        log_abbreviated("[RAW SERVER] cmd: '" + cmd + "', data size = " + JSON.stringify(data).length);

        if (Date.now() - last_video_progress_msg_ts > 5000) {
          $video_progress_msg = null; // timeout progress message after a while
        }

        switch (cmd)
        {
          case 'welcome':
            //log_abbreviated("[SERVER] welcome: " + JSON.stringify(data));
            $cur_username = data.username;
            $cur_user_id = data.user_id
            break;

          case 'error':
            console.log("[SERVER ERROR]: " + JSON.stringify(data));
            acts.add({mode: 'danger', message: data.msg, lifetime: 5});
            break;

          case 'user_videos':
            log_abbreviated("[SERVER] user_videos: " + JSON.stringify(data));
            $all_my_videos = data.videos;
            console.log("Got " + $all_my_videos.length + " videos");
            console.log($all_my_videos);
            break;

          case 'message':
            log_abbreviated("[SERVER] message: " + JSON.stringify(data));
            if ( data.event_name == 'progress' ) {
              if (data.ref_video_hash == $video_hash) {
                $video_progress_msg = data.message;
                last_video_progress_msg_ts = Date.now();
              }
            }
            else if ( data.event_name == 'video_updated' ) {
              refresh_my_videos();
            }
            else {
              $user_messages = $user_messages.filter((m) => m.id != data.id);
              if (data.created) { $user_messages.push(data); }
              $user_messages = $user_messages.sort((a, b) => a.id > b.id ? -1 : a.id < b.id ? 1 : 0);
              if (!data.seen) {
                const severity = (data.event_name == 'error') ? 'danger' : 'info';
                acts.add({mode: severity, message: data.message, lifetime: 5});
                if (severity == 'info') {
                  refresh_my_videos();
              }};
            }
            break;

          case 'open_video':
            log_abbreviated("[SERVER] open_video: " + JSON.stringify(data));
            $video_url = data.video_url;
            $video_hash = data.video_hash;
            $video_fps = data.fps;
            $video_title = data.title;
            $all_comments = [];
            if ($collab_id)
              ws_emit('join_collab', {collab_id: $collab_id, video_hash: $video_hash});
            break;

          case 'new_comment':
            log_abbreviated("[SERVER] new_comment: " + JSON.stringify(data));
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

              //console.log("[SERVER] new_comment id=" + data.comment_id + " parent_id=" + data.parent_id + " tc=" + data.timecode + " comment=" + data.comment);
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
                log_abbreviated("Comment not for this video. Ignoring.");
              }
            }
            break;

          case 'del_comment':
            //log_abbreviated("[SERVER] del_comment: " + data.comment_id);
            $all_comments = $all_comments.filter((c) => c.id != data.comment_id);
            break;

          case 'collab_cmd':
            log_abbreviated("[SERVER] collab_cmd: " + JSON.stringify(data));
            if (!data.paused) {
              video_player.collabPlay(data.seek_time);
            } else {
              video_player.collabPause(data.seek_time, data.drawing);
            }
            if (last_collab_controlling_user != data.from_user) {
              last_collab_controlling_user = data.from_user;
              acts.add({mode: 'info', message: last_collab_controlling_user + " is controlling", lifetime: 5});
            }
            break;

          default:
            log_abbreviated("[SERVER] UNKNOWN CMD '"+data.cmd+"': " + JSON.stringify(data));
            break;
        }
      });
    });

  }

  function onClickDeleteVideo(video_hash: string, video_name: string) {
    log_abbreviated("onClickDeleteVideo: " + video_hash + " / " + video_name);
    if (confirm("Are you sure you want to delete '" + video_name + "'?")) {
      ws_emit('del_video', {video_hash: video_hash});

      // After 2 seconds, refresh the list of videos
      function refresh_my_videos() { ws_emit('list_my_videos', {}); }
      setTimeout(refresh_my_videos, 2000);
    }
  }

  function onClickRenameVideo(video_hash: string, video_name: string) {
    log_abbreviated("onClickRenameVideo: " + video_hash + " / " + video_name);
    let new_name = prompt("Rename video to:", video_name);
    if (new_name) {
      ws_emit('rename_video', {video_hash: video_hash, new_name: new_name});
      ws_emit('list_my_videos', {});
    }
  }

function installThumbScrubber(e: MouseEvent, item: object)
{
  let sheet_cols = item.thumb_sheet_cols;
  let sheet_rows = item.thumb_sheet_rows;
  let bgImg =  new Image();

  bgImg.onload = (le) => {
    // Total size of sprite sheet in pixels
    let sheet_w_px = le.target.naturalWidth;
    let sheet_h_px = le.target.naturalHeight;

    // Size of one frame in pixels
    let frame_width = sheet_w_px / sheet_cols;
    let frame_height = sheet_h_px / sheet_rows;

    // Size of current div (that shows the sprite sheet) in pixels
    let div_w_px = e.target.clientWidth;
    let div_h_px = e.target.clientHeight;

    // Switch background image to the now loaded sprite sheet
    e.target.style.backgroundRepeat = 'no-repeat';
    e.target.style.backgroundImage = 'url(' + bgImg.src + ')';

    // Scale the sprite sheet so one frame fits in the div
    let scaled_bgr_w = (div_w_px / frame_width) * sheet_w_px;
    let scaled_bgr_h = (div_h_px / frame_height) * sheet_h_px;
    e.target.style.backgroundSize = scaled_bgr_w + 'px ' + scaled_bgr_h + 'px';

    function show_frame(frame_idx) {
      let frame_xi = frame_idx % sheet_cols;
      let frame_yi = Math.floor(frame_idx / sheet_cols);

      let frame_left = scaled_bgr_w * (frame_xi / sheet_cols);
      let frame_top = scaled_bgr_h * (frame_yi / sheet_rows);

      e.target.style.backgroundPosition = '-' + frame_left + 'px -' + frame_top + 'px';
    }

    // Show first frame at first
    show_frame(0);

    // Scrub sheet on mouse move
    e.target.onmousemove = (e) => {
      let frame_idx = Math.floor((e.offsetX / e.target.clientWidth) * (sheet_cols * sheet_rows));
      show_frame(frame_idx);
    }
  };

  // Start loading the sprite sheet
  bgImg.src = item.thumb_sheet_url;
}

function removeThumbScrubber(e: MouseEvent, item: object)
{
  // Restore original background image (item.thumb_url)
  e.target.onmousemove = null;
  e.target.onload = null;
  e.target.style.backgroundImage = 'url(' + item.thumb_url + ')';
  e.target.style.backgroundPosition = '0 0';
  e.target.style.backgroundSize = '100% 100%';
}



</script>

<svelte:window on:popstate={popHistoryState}/>
<main>
<div class="flex flex-col w-screen h-screen {debug_layout?'border-2 border-yellow-300':''}">
    <div class="flex-none w-full"><NavBar on:clear-all={onClearAll} on:basic-auth-logout={disconnect} /></div>
    <div class="flex-grow w-full overflow-auto {debug_layout?'border-2 border-cyan-300':''}">
        <Notifications />

        {#if !ui_connected_state }

          <!-- ========== "connecting" spinner ============= -->
          <div transition:fade class="w-full h-full text-5xl text-slate-600 align-middle text-center">
            <h1 class="m-16" style="font-family: 'Yanone Kaffeesatz', sans-serif;">
              Connecting server...
            </h1>
            <div class="fa-2x block">
              <i class="fas fa-spinner connecting-spinner"></i>
            </div>

          </div>

        {:else if $video_hash}

          <!-- ========== video review widgets ============= -->
          <div transition:slide class="flex h-full w-full {debug_layout?'border-2 border-blue-700':''}">

            <div transition:slide class="flex-1 flex flex-col {debug_layout?'border-2 border-purple-600':''}">
              <div class="flex-1 bg-cyan-900">
                <VideoPlayer
                  bind:this={video_player} src={$video_url}
                  on:seeked={onVideoSeeked}
                  on:collabReport={onCollabReport}
                  on:commentPinClicked={onCommentPinClicked}
                  />
              </div>
              <div class="flex-none w-full p-2 {debug_layout?'border-2 border-green-500':''}">
                <CommentInput bind:this={comment_input} on:button-clicked={onCommentInputButton} />
              </div>
            </div>

            {#if $all_comments.length > 0}
            <!-- ========== comment sidepanel ============= -->
            <div id="comment_list" transition:fade class="flex-none w-72 basis-128 bg-gray-900 py-2 px-2 space-y-2 ml-2 overflow-y-auto">
                {#each $all_comments as item}
                  <CommentCard {...item} on:display-comment={onDisplayComment} on:delete-comment={onDeleteComment} on:reply-to-comment={onReplyComment} on:edit-comment={onEditComment}/>
                {/each}
            </div>
            {/if}
          </div>

          {#if $collab_id && !collab_dialog_ack}
          <div class="fixed top-0 left-0 w-full h-full flex justify-center items-center">
              <div class="bg-gray-900 text-white p-4 rounded-md shadow-lg text-center leading-loose">
                <p class="text-xl text-green-500">Collaborative viewing session active.</p>
                <p class="">Session ID is <code class="text-green-700">{$collab_id}</code></p>
                <p class="">Actions like seek, play and draw are mirrored to all participants.</p>
                <p class="">To invite people, copy browser URL and send it to them.</p>
                <p class="">Exit by clicking the green icon in header.</p>
                <button class="bg-gray-800 hover:bg-gray-700 text-green m-2 p-2 rounded-md shadow-lg" on:click|preventDefault="{()=>collab_dialog_ack=true}">Understood</button>
              </div>
          </div>
          {/if}

        {:else}

          {#if upload_url }
          <div class="m-6">
            <FileUpload post_url={upload_url}/>
          </div>
          {/if}

          <!-- ========== video listing ============= -->
          <div class="m-6 text">
            <h1 class="text-4xl m-6">
              {#if $all_my_videos.length==0}
                You have no videos.
              {/if}
            </h1>
            <div class="gap-8">
              {#each $all_my_videos as item}
              <div class="bg-slate-600 w-80 h-20 rounded-md p-2 m-1 mx-6 overflow-clip inline-block cursor-pointer"
                  on:click|preventDefault={ () => onClickVideo(item.video_hash) }
                  on:keypress={(e) => { if (e.key === 'Enter') { onClickVideo(item.video_hash) }}}
                  >

                {#if item.thumb_url}
                  <!-- hover mouse to scrub thumb sheet -->
                  <div class="w-[7.111rem] h-[4rem] float-left mr-2 bg-gray-900 rounded-md overflow-hidden"
                    style="background-image: url('{item.thumb_url}'); background-size: cover; background-position: 0 0;"
                    on:focus={()=>{}}
                    on:blur={()=>{}}
                    on:mouseover={(e) => installThumbScrubber(e, item)}
                    on:mouseout={(e) => removeThumbScrubber(e, item)}
                  >
                  </div>
                {/if}

                <span class="text-amber-400 text-xs pr-2 border-r border-gray-400">{item.added_time}</span>
                <span class="text-amber-500 font-mono text-xs pr-2">{item.video_hash}</span>
                <VideoListPopup
                  onDel={() => { onClickDeleteVideo(item.video_hash, item.title) }}
                  onRename={() => { onClickRenameVideo(item.video_hash, item.title) }} />
                <div class="leading-none"><a href="/?vid={item.video_hash}" title="{item.title}" class="break-all text-xs">{item.title}</a></div>
              </div>
              {/each}
            </div>

            {#if $user_messages.length>0}
              <h1 class="text-2xl m-6 mt-12 text-slate-500">
                  Latest messages
              </h1>
              <div class="gap-4 max-h-56 overflow-y-auto border-l px-2 border-gray-900">
                {#each $user_messages as msg}
                  <UserMessage {msg} />
                {/each}
              </div>
            {/if}

          </div>

        {/if}
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
}

/*
::-webkit-scrollbar {
    display: none;
}
body {
    -ms-overflow-style: none;
    scrollbar-width: none;
}
*/

</style>

