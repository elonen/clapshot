<script lang="ts">
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import {fade, slide} from "svelte/transition";

  import * as Proto3 from '@clapshot_protobuf/typescript';

  import {all_comments, cur_username, cur_user_id, video_is_ready, video_url, video_hash, video_fps, video_title, cur_page_items, user_messages, video_progress_msg, collab_id, user_menu_items, server_defined_actions} from '@/stores';
  import {IndentedComment, type UserMenuItem} from "@/types";

  import CommentCard from '@/lib/CommentCard.svelte'
  import NavBar from '@/lib/NavBar.svelte'
  import CommentInput from '@/lib/CommentInput.svelte';
  import UserMessage from '@/lib/UserMessage.svelte';
  import FileUpload from '@/lib/FileUpload.svelte';
  import VideoPlayer from '@/lib/VideoPlayer.svelte';
  import type {VideoListDefItem} from "@/lib/video_list/types";
  import VideoList from "@/lib/video_list/VideoList.svelte";


  let video_player: VideoPlayer;
  let comment_input: CommentInput;
  let debug_layout: boolean = false;
  let ui_connected_state: boolean = false; // true if UI should look like we're connected to the server

  let last_video_progress_msg_ts = Date.now();  // used to hide video_progress_msg after a few seconds

  let collab_dialog_ack = false;  // true if user has clicked "OK" on the collab dialog
  let last_collab_controlling_user: string | null = null;    // last user to control the video in a collab session

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
    video_player.seekToSMPTE(e.detail.timecode);
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
      id: e.detail.id,
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
      id: e.detail.id,
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
    history.pushState('/', '', '/');  // Clear URL
    closeVideo();
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
    let c = $all_comments.find(c => c.comment.id == comment_id);
    if (c) {
      onDisplayComment({detail: {timecode: c.comment.timecode, drawing: c.comment.drawing}});
      let card = document.getElementById("comment_card_" + comment_id);
      if (card) {
        card.scrollIntoView({behavior: "smooth", block: "center", inline: "nearest"});
        setTimeout(() => { card?.classList.add("highlighted_comment"); }, 500);
        setTimeout(() => { card?.classList.remove("highlighted_comment"); }, 3000);
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
      console.error("Got UNKNOWN URL parameter: '" + key + "'. Value= " + value);
      acts.add({mode: 'warn', message: "Unknown URL parameter: '" + key + "'", lifetime: 5});
    }
  });

  $video_hash = urlParams.get('vid');
  const prev_collab_id = $collab_id;
  $collab_id = urlParams.get('collab');
  if ($video_hash) {
    // console.log("Video hash: " + video_hash);
    if ($collab_id)
      history.pushState($video_hash, '', '/?vid='+$video_hash+'&collab='+$collab_id);
    else
      history.pushState($video_hash, '', '/?vid='+$video_hash);
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
          $user_menu_items = [...$user_menu_items, {label: "Logout", type: "logout-basic-auth"} as UserMenuItem];
        }
      })
      .catch(error => {
          console.error("Failed to read config:", error)
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
        console.error("Exception in Websocket handler: ", e);
        console.log(e.stack);
        acts.add({mode: 'danger', message: 'Client error: ' + e, lifetime: 5});
      }
    }

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
          {
            //log_abbreviated("[SERVER] welcome: " + JSON.stringify(data));
            $cur_username = data.username;
            $cur_user_id = data.user_id
            break;
          }

          case 'error':
          {
            console.error("[SERVER ERROR]: ", data);
            acts.add({mode: 'danger', message: data.msg, lifetime: 5});
            break;
          }

          case 'show_page':
          {
            log_abbreviated("[SERVER] show_page: " + JSON.stringify(data));
            $cur_page_items = data.page_items.map(
                (pi: any) => Proto3.PageItem.fromJSON(pi));
            break;
          }

          case 'define_actions':
          {
            log_abbreviated("[SERVER] define_actions: " + JSON.stringify(data));
            $server_defined_actions = Proto3.ClientDefineActionsRequest.fromJSON(data).actions;
            break;
          }

          case 'message':
          {
            log_abbreviated("[SERVER] message: " + JSON.stringify(data));
            const msg = Proto3.UserMessage.fromJSON(data);

            if ( msg.type === Proto3.UserMessage_Type.PROGRESS ) {
              if (msg.refs?.videoHash == $video_hash) {
                $video_progress_msg = msg.message;
                last_video_progress_msg_ts = Date.now();
              }
            }
            else if ( msg.type === Proto3.UserMessage_Type.VIDEO_UPDATED ) {
              refresh_my_videos();
            }
            else {
              $user_messages = $user_messages.filter((m) => m.id != msg.id);
              if (msg.created) { $user_messages.push(msg); }
              if (!msg.seen) {
                const severity = (msg.type == Proto3.UserMessage_Type.ERROR) ? 'danger' : 'info';
                acts.add({mode: severity, message: msg.message, lifetime: 5});
                if (severity == 'info') {
                  refresh_my_videos();
              }};
            }
            break;
          }

          case 'open_video':
          {
            log_abbreviated("[SERVER] open_video: " + JSON.stringify(data));
            let v = Proto3.Video.fromJSON(data);
            try
            {
              if (!v.playbackUrl) throw Error("No playback URL");
              if (!v.duration) throw Error("No duration");
              if (!v.title) throw Error("No title");

              $video_url = v.playbackUrl;
              $video_hash = v.videoHash;
              $video_fps = parseFloat(v.duration.fps);
              if (isNaN($video_fps)) throw Error("Invalid FPS");
              $video_title = v.title;
              $all_comments = [];

              if ($collab_id)
                ws_emit('join_collab', {collab_id: $collab_id, video_hash: $video_hash});
              else
                history.pushState($video_hash, '', '/?vid='+$video_hash);  // Point URL to video
            } catch(error) {
              acts.add({mode: 'danger', message: 'Bad video open request. See log.', lifetime: 5});
              console.error("Invalid video open request. Error: ", error, "Data: ", data);
            }
            break;
          }

          case 'new_comment':
          {
            log_abbreviated("[SERVER] new_comment: " + JSON.stringify(data));
            {
              let new_comment = Proto3.Comment.fromJSON(data);

              function indentCommentTree(items: IndentedComment[]): IndentedComment[]
              {
                let rootComments = items.filter(item => item.comment.parentId == null);
                rootComments.sort((a, b) => (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0));

                // Recursive DFS function to traverse and build the ordered list
                function dfs(c: IndentedComment, depth: number, result: IndentedComment[]): void {
                  if (result.find((it) => it.comment.id === c.comment.id)) return;  // already added, cut infinite loop
                  result.push({ ...c, indent: depth });
                  let children = items.filter(item => (item.comment.parentId === c.comment.id));
                  children.sort((a, b) => (a.comment.created?.getTime() ?? 0) - (b.comment.created?.getTime() ?? 0));
                  for (let child of children)
                    dfs(child, depth + 1, result);
                }

                let res: IndentedComment[] = [];
                rootComments.forEach((c) => dfs(c, 0, res));

                // Add any orphaned comments to the end (we may receive them out of order)
                items.forEach((c) => {
                  if (!res.find((it) => it.comment.id === c.comment.id))
                    res.push(c);
                });
                return res;
              }

              if (new_comment.videoHash == $video_hash)
              {
                $all_comments.push({
                  comment: new_comment,
                  indent: 0
                });
                $all_comments = indentCommentTree($all_comments);
              } else {
                log_abbreviated("Comment not for this video. Ignoring.");
              }
            }
            break;
          }

          case 'del_comment':
          {
            $all_comments = $all_comments.filter((c) => c.comment.id != data.id);
            break;
          }

          case 'collab_cmd':
          {
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
          }

          default:
          {
            console.error("[SERVER] UNKNOWN CMD '"+data.cmd+"'", data);
            break;
          }
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

  function onMoveItemsToFolder(_e: {detail: {folder_id: any; items: any[]}}) {
    console.error("NOT IMPLEMENTED! onMoveItemsToFolder: " + _e.detail.folder_id, "items:", _e.detail.items);
  }

  function onReorderItems(e: any) {
    console.error("NOT IMPLEMENTED! onReorderItems: ", e.detail);
  }

  function openVideoListItem(e: { detail: Proto3.PageItem_FolderListing_Item}): void {
    let it = e.detail;
    if (it.openAction) {
      if ( it.openAction.lang == Proto3.ScriptCall_Lang.JAVASCRIPT )
        callOrganizerScript(it.openAction.code, [it]);
      else {
        console.error("BUG: Unsupported Organizer script language: " + it.openAction.lang);
        acts.add({mode: 'error', message: "BUG: Unsupported script lang. See log.", lifetime: 5});
      }
    } else {
      console.error("No openAction script for item: " + it);
      acts.add({mode: 'error', message: "No open action for item. See log.", lifetime: 5});
    }
  }

  // ------------

  /// Execute a script from Organizer (or server, if Organizer is not connected)
  function callOrganizerScript(code: string|undefined, items: any[]): void {
    if (!code) {
      console.log("callOrganizerScript called with empty code. Ignoring.");
      return;
    }
    async function call_server(cmd: string, args: Object): Promise<void> { ws_emit(cmd, args); }
    async function call_organizer(cmd: string, args: Object): Promise<void> { ws_emit("organizer", {cmd, args}); }
    async function alert(msg: string): Promise<void> { window.alert(msg); }
    async function prompt(msg: string, default_value: string): Promise<string|null> { return window.prompt(msg, default_value); }
    async function confirm(msg: string): Promise<boolean> { return window.confirm(msg); }

    const AsyncFunction = async function () {}.constructor;
    // @ts-ignore
    let script_fn = new AsyncFunction("call_server", "call_organizer", "alert", "prompt", "confirm", "items", code);

    console.log("Calling organizer script. Code = ", code, "items=", items);

    script_fn(call_server, call_organizer, alert, prompt, confirm, items)
      .catch((e: any) => {
        console.error("Error in organizer script:", e);
        acts.add({mode: 'error', message: "Organizer script error. See log.", lifetime: 5});
    });
  }

  function onVideoListPopupAction(e: { detail: { action: Proto3.ActionDef, items: VideoListDefItem[] }})
  {
    let {action, items} = e.detail;
    let items_objs = items.map((it) => it.obj);
    console.log("onVideoListPopupAction: ", action, items_objs);
    callOrganizerScript(action.action?.code, items_objs);
  }

// ------------

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
                {#each $all_comments as it}
                  <CommentCard
                    indent={it.indent}
                    comment={it.comment}
                    on:display-comment={onDisplayComment}
                    on:delete-comment={onDeleteComment}
                    on:reply-to-comment={onReplyComment}
                    on:edit-comment={onEditComment}/>
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

          <!-- ========== page components ============= -->
          <div class="organizer_page">
            {#each $cur_page_items as item}
              {#if item.html }
                <div>
                    {@html item.html}
                </div>
              {:else if item.folderListing}
                <div class="my-6">
                  <VideoList items={item.folderListing.items.map((it)=>({
                      id: (it.video?.videoHash ?? it.folder?.id ?? "[BUG: BAD ITEM TYPE]"),
                      obj: it }))}
                    on:open-item={openVideoListItem}
                    on:reorder-items={onReorderItems}
                    on:move-to-folder={onMoveItemsToFolder}
                    on:popup-action={onVideoListPopupAction}
                    />
                </div>
              {/if}
            {/each}
          </div>

          <!-- ========== upload widget ============= -->
          <div class="m-6 h-24 border-4 border-dashed border-gray-700">
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

/* Make all headings in organizer page bigger */
:global(div.organizer_page){
  margin: 2em;
}

:global(.organizer_page h2){
  font-size: 200%;
}

</style>
