<script lang="ts">
  import {fade, slide} from "svelte/transition";
  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import CommentInput from './lib/CommentInput.svelte';
  import UserMessage from './lib/UserMessage.svelte';
  import FileUpload from './lib/FileUpload.svelte';
  import type VideoListVideoTile from "./lib/video_list/VideoListVideoTile.svelte";
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import VideoPlayer from './lib/VideoPlayer.svelte';

  import {all_comments, cur_username, cur_user_id, video_is_ready, video_url, video_hash, video_fps, video_title, all_my_videos, user_messages, video_progress_msg, collab_id, user_menu_items} from './stores.js';

  import {VideoListDefItem, VideoListVideoDef, VideoListFolderDef, videoOrFolder} from "./lib/video_list/types";
  import VideoList from "./lib/video_list/VideoList.svelte";

  import type { ClapshotVideoJson } from "./lib/video_list/types";
  import VideoListFolder from "./lib/video_list/VideoListFolder.svelte";
  import { get } from "svelte/store";
  //import type { ClapshotCommentJson } from "./lib/video_list/types";

  let videoTiles: VideoListVideoTile[] = []; 

  let video_player: VideoPlayer;
  let comment_input: CommentInput;
  let debug_layout: boolean = false;
  let ui_connected_state: boolean = false; // true if UI should look like we're connected to the server

  let last_video_progress_msg_ts = Date.now();  // used to hide video_progress_msg after a few seconds

  let collab_dialog_ack = false;  // true if user has clicked "OK" on the collab dialog
  let last_collab_controlling_user = null;    // last user to control the video in a collab session

  function log_abbreviated(...strs: any[]) {
      const max_len = 180;
      let abbreviated: string[] = [];
      for (let i = 0; i < strs.length; i++) {
        let str = (typeof strs[i] == "string" || typeof strs[i] == "number" || typeof strs[i] == "boolean")
          ? String(strs[i])
          : JSON.stringify(strs[i]);
        abbreviated[i] = (str.length > max_len) ? (str.slice(0, max_len) + "(...)") : str;
      }
      console.log(...abbreviated);
  }

  // Messages from CommentInput component
  function onCommentInputButton(e: any) {
    if (e.detail.action == "send")
    {
      if (e.detail.comment_text != "")
      {
        ws_emit('add_comment', {
          video_hash: $video_hash,
          parent_id: null,            // TODO: parent id here
          comment: e.detail.comment_text,
          drawing: video_player.getScreenshot(),
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

  function onDisplayComment(e: any) {
    video_player.seekTo(e.detail.timecode, 'SMPTE');
    // Close draw mode while showing (drawing from a saved) comment
    video_player.onToggleDraw(false);
    comment_input.forceDrawMode(false);
    if (e.detail.drawing)    
      video_player.setDrawing(e.detail.drawing);
    if ($collab_id) {
      log_abbreviated("Collab: onDisplayComment. collab_id: '" + $collab_id + "'");
      ws_emit('collab_report', {paused: true, seek_time: video_player.getCurTime(), drawing: e.detail.drawing});
    }
  }

  function onDeleteComment(e: any) {
    ws_emit('del_comment', {
      comment_id: e.detail.comment_id,
    });
  }

  function onReplyComment(e: any) {
    ws_emit('add_comment', {
        video_hash: $video_hash,
        parent_id: e.detail.parent_id,
        comment: e.detail.comment_text,
      });
  }

  function onEditComment(e: any) {
    ws_emit('edit_comment', {
      comment_id: e.detail.comment_id,
      comment: e.detail.comment_text,
    });
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

  function onClearAll(_e: any) {
    history.pushState('/', null, '/');  // Clear URL
    closeVideo();
  }

  function onRequestFolderOpen(e: any) {
    alert("TODO: open folder " + e.detail.folder_id);
  }


  function onVideoSeeked(_e: any) {
    comment_input.forceDrawMode(false);  // Close draw mode when video frame is changed
  }

  function onCollabReport(e: any) {
    if ($collab_id)
      ws_emit('collab_report', {paused: e.detail.paused, seek_time: e.detail.seek_time, drawing: e.detail.drawing});
  }

  function onCommentPinClicked(e: any) {
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

  function popHistoryState(e: any) {
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


  // Called after we get the API URL from the server.
  function connect_websocket(ws_url: string)
  {
    if (!ws_url)
      throw Error("API URL not specified in config file");

    console.log("...CONNECTING to WS API: " + ws_url);
    ws_socket = new WebSocket(ws_url);

    // Handle connection opening
    ws_socket.addEventListener("open", function (_event) {
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

    function handle_with_errors(func: { (): any; }): any {
      try {
        return func();
      } catch (e: any) {
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
    ws_socket.addEventListener("close", function (_event) {
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
            //console.log($all_my_videos);
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
              function reorder_comments(old_order: any[]) {
                // Helper to show comment threads in the right order and with correct indentation
                let old_sorted = old_order.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
                let new_order = [];
                function find_insert_position_and_indent(parent_id: number)
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

  function onRequestVideoDelete(video_hash: string, video_name: string) {
    log_abbreviated("onRequestVideoDelete: " + video_hash + " / " + video_name);
    ws_emit('del_video', {video_hash});
    ws_emit('list_my_videos', {});
  }

  function onRequestVideoRename(video_hash: string, video_name: string) {
    log_abbreviated("onRequestVideoRename: " + video_hash + " / " + video_name);
    let new_name = prompt("Rename video to:", video_name);
    if (new_name) {
      ws_emit('rename_video', {video_hash, new_name});
      ws_emit('list_my_videos', {});
    }
  }

  function onRequestFolderDelete(folder_id: string) {
    alert("NOT IMPLEMENTED: delete folder: " + folder_id);
  }

  function onRequestFolderRename(folder_id: string, folder_name: string) {
    alert("NOT IMPLEMENTED: rename folder: " + folder_id + " / " + folder_name);
  }

  function onMoveItemsToFolder(_e: {detail: {folder_id: any; items: any[]}}) {
    console.log("NOT IMPLEMENTED! onMoveItemsToFolder: " + _e.detail.folder_id, "items:", _e.detail.items);
  }

  function TEMP_getVideoListItems(items: any[]): VideoListDefItem[] {
    let res: VideoListDefItem[] = items.map((it) => new VideoListVideoDef(it));
    let copy = [...res];
    let fld1 = new VideoListFolderDef("test1", "Test Folder 1", copy);
    let fld2 = new VideoListFolderDef("test2", "Test Folder 2", []);
    res.push(fld1);
    res.push(fld2);
    return res;
  }

  function TEMP_reorderItems(e: any) {
    console.log("TEMP_reorderItems:", e.detail.items);
  }

  function openVideoListItem(e: { detail: { video: any; folder: any }}): void {
    if (e.detail.video) {
      let new_video_hash = e.detail.video.video_hash;
      ws_emit('open_video', {video_hash: new_video_hash});
      history.pushState(new_video_hash, null, '/?vid='+new_video_hash);  // Point URL to video
    } else if (e.detail.folder) {
      alert("Folder open: " + e.detail.folder.folder_id);
    }
  }

  function onVideoListPopupAction(e: { detail: { action: string, items: VideoListDefItem[] }}) {
    let {action, items} = e.detail;
    switch (action) {
      case 'delete':
        let subject = items.length == 1 ? "this item" : items.length + " items";
        if (confirm("Are you sure you want to DELETE " + subject + "?")) {
          items.forEach((it) => {
            let {video, folder} = videoOrFolder(it);
            if (video)
              onRequestVideoDelete(video.video_hash, video.title);
            else if (folder)
              onRequestFolderDelete(folder.folder_id);
        })}
        break;
      case 'rename':
        if (items.length == 1) {
          let {video, folder} = videoOrFolder(items[0]);
          if (video)
            onRequestVideoRename(video.video_hash, video.title);
          else if (folder)
            onRequestFolderRename(folder.folder_id, folder.name);
        } else {
          alert("Can only rename one item at a time.");
        }
        break;
      default:
        alert("NOT IMPLEMENTED (TODO: push this action blindly to server?): " + action);
    }
  }

</script>

<svelte:window on:popstate={popHistoryState}/>
<main>
<span id="popup-container"></span>
<div class="flex flex-col bg-[#101016] w-screen h-screen {debug_layout?'border-2 border-yellow-300':''}">
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

          <!-- ========== video listing ============= -->        
          <div class="m-6 text">
            <h1 class="text-4xl m-6">
              {#if $all_my_videos.length>0}
                All your videos
              {:else}
                You have no videos.
              {/if}
            </h1>

          </div>
          
          <div class="m-4">
            <VideoList items={TEMP_getVideoListItems($all_my_videos)} 
              on:open-item={openVideoListItem}
              on:reorder-items={TEMP_reorderItems}
              on:move-to-folder={onMoveItemsToFolder}
              on:popup-action={onVideoListPopupAction}
              />

            <div class="w-full my-4 h-24 border-4 border-dashed border-gray-700">
              <FileUpload post_url={upload_url}>
                <div class="flex flex-col justify-center items-center h-full">
                  <div class="text-2xl text-gray-700">
                    <i class="fas fa-upload"></i>
                  </div>
                  <div class="text-xl text-gray-700">
                    Drop video files here to upload
                  </div>
                </div>
              </FileUpload>
            </div>
          </div>

          <div>
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

</style>

