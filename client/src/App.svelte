<script lang="ts">
import {Notifications, acts} from '@tadashi/svelte-notification'
import {fade, slide} from "svelte/transition";

import * as Proto3 from '@clapshot_protobuf/typescript';

import {allComments, curUsername, curUserId, videoIsReady, videoUrl, videoHash, videoFps, videoTitle, curPageItems, userMessages, videoProgressMsg, collabId, userMenuItems, serverDefinedActions} from '@/stores';
import {IndentedComment, type UserMenuItem} from "@/types";

import CommentCard from '@/lib/CommentCard.svelte'
import NavBar from '@/lib/NavBar.svelte'
import CommentInput from '@/lib/CommentInput.svelte';
import UserMessage from '@/lib/UserMessage.svelte';
import FileUpload from '@/lib/FileUpload.svelte';
import VideoPlayer from '@/lib/VideoPlayer.svelte';
import type {VideoListDefItem} from "@/lib/video_list/types";
import VideoList from "@/lib/video_list/VideoList.svelte";


let videoPlayer: VideoPlayer;
let commentInput: CommentInput;
let debugLayout: boolean = false;
let uiConnectedState: boolean = false; // true if UI should look like we're connected to the server

let lastVideoProgressMsgTime = Date.now();  // used to hide video_progress_msg after a few seconds

let collabDialogAck = false;  // true if user has clicked "OK" on the collab dialog
let lastCollabControllingUser: string | null = null;    // last user to control the video in a collab session

function logAbbrev(...strs: any[]) {
    const maxLen = 180;
    let abbreviated: string[] = [];
    for (let i = 0; i < strs.length; i++) {
        let str = (typeof strs[i] == "string" || typeof strs[i] == "number" || typeof strs[i] == "boolean")
        ? String(strs[i])
        : JSON.stringify(strs[i]);
        abbreviated[i] = (str.length > maxLen) ? (str.slice(0, maxLen) + "(...)") : str;
    }
    console.log(...abbreviated);
}

// Messages from CommentInput component
function onCommentInputButton(e: any) {
    if (e.detail.action == "send")
    {
        if (e.detail.comment_text != "")
        {
            wsEmit('add_comment', {
                video_hash: $videoHash,
                parent_id: null,            // TODO: parent id here
                comment: e.detail.comment_text,
                drawing: videoPlayer.getScreenshot(),
                timecode: e.detail.is_timed ? videoPlayer.getCurTimecode() : "",
            });
        }
    }
    else if (e.detail.action == "color_select") {
        videoPlayer.onColorSelect(e.detail.color);
    }
    else if (e.detail.action == "draw") {
        videoPlayer.onToggleDraw(e.detail.is_draw_mode);
    }
    else if (e.detail.action == "undo") {
        videoPlayer.onDrawUndo();
    }
    else if (e.detail.action == "redo") {
        videoPlayer.onDrawRedo();
    }
}

function onDisplayComment(e: any) {
    videoPlayer.seekToSMPTE(e.detail.timecode);
    // Close draw mode while showing (drawing from a saved) comment
    videoPlayer.onToggleDraw(false);
    commentInput.forceDrawMode(false);
    if (e.detail.drawing)
    videoPlayer.setDrawing(e.detail.drawing);
    if ($collabId) {
        logAbbrev("Collab: onDisplayComment. collab_id: '" + $collabId + "'");
        wsEmit('collab_report', {paused: true, seek_time: videoPlayer.getCurTime(), drawing: e.detail.drawing});
    }
}

function onDeleteComment(e: any) {
    wsEmit('del_comment', {
        id: e.detail.id,
    });
}

function onReplyComment(e: any) {
    wsEmit('add_comment', {
        video_hash: $videoHash,
        parent_id: e.detail.parent_id,
        comment: e.detail.comment_text,
    });
}

function onEditComment(e: any) {
    wsEmit('edit_comment', {
        id: e.detail.id,
        comment: e.detail.comment_text,
    });
}

function closeVideo() {
    // Close current video, list all user's own videos.
    // This is called from onClearAll event and history.back()
    console.log("closeVideo");
    wsEmit('leave_collab', {});
    $collabId = null;
    $videoHash = null;
    $videoUrl = null;
    $videoFps = null;
    $videoTitle = null;
    $allComments = [];
    $videoIsReady = false;
    wsEmit('list_my_videos', {});
    wsEmit('list_my_messages', {});
}

function onClearAll(_e: any) {
    history.pushState('/', '', '/');  // Clear URL
    closeVideo();
}

function onVideoSeeked(_e: any) {
    commentInput.forceDrawMode(false);  // Close draw mode when video frame is changed
}

function onCollabReport(e: any) {
    if ($collabId)
    wsEmit('collab_report', {paused: e.detail.paused, seek_time: e.detail.seek_time, drawing: e.detail.drawing});
}

function onCommentPinClicked(e: any) {
    // Find corresponding comment in the list, scroll to it and highlight
    let commentId = e.detail.id;
    let c = $allComments.find(c => c.comment.id == commentId);
    if (c) {
        onDisplayComment({detail: {timecode: c.comment.timecode, drawing: c.comment.drawing}});
        let card = document.getElementById("comment_card_" + commentId);
        if (card) {
            card.scrollIntoView({behavior: "smooth", block: "center", inline: "nearest"});
            setTimeout(() => { card?.classList.add("highlighted_comment"); }, 500);
            setTimeout(() => { card?.classList.remove("highlighted_comment"); }, 3000);
        }
    }
}

function popHistoryState(e: any) {
    if (e.state && e.state !== '/')
    wsEmit('open_video', {video_hash: e.state});
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

$videoHash = urlParams.get('vid');
const prevCollabId = $collabId;
$collabId = urlParams.get('collab');
if ($videoHash) {
    // console.log("Video hash: " + video_hash);
    if ($collabId)
    history.pushState($videoHash, '', '/?vid='+$videoHash+'&collab='+$collabId);
    else
    history.pushState($videoHash, '', '/?vid='+$videoHash);
}

let uploadUrl: string = "";


// -------------------------------------------------------------
// Websocket messaging
// -------------------------------------------------------------

// Read config from HTTP server first
const CONF_FILE = "clapshot_client.conf.json";
function handleErrors(response: any) {
    if (!response.ok)
    throw Error("HTTP error: " + response.status);
    return response;
}
fetch(CONF_FILE)
.then(handleErrors)
.then(response => response.json())
.then(json => {
    // Check that we have all the expected config lines
    const expected = ["ws_url", "upload_url", "user_menu_extra_items", "user_menu_show_basic_auth_logout"];
    for (let key of expected) {
        if (!(key in json))
            throw Error("Missing key '" + key + "' in client config file '" + CONF_FILE + "'");
    }

    uploadUrl = json.upload_url;
    connectWebsocket(json.ws_url);

    $userMenuItems = json.user_menu_extra_items;
    if (json.user_menu_show_basic_auth_logout) {
        $userMenuItems = [...$userMenuItems, {label: "Logout", type: "logout-basic-auth"} as UserMenuItem];
    }
})
.catch(error => {
    console.error("Failed to read config:", error)
    acts.add({mode: 'danger', message: "Failed to read config. " + error, lifetime: 50});
});


let videoListRefreshScheduled = false;
function refreshMyVideos()
{
    if (!videoListRefreshScheduled) {
        videoListRefreshScheduled = true;
        setTimeout(() => {
            videoListRefreshScheduled = false;
            wsEmit('list_my_videos', {});
        }, 500);
    }
}



let wsSocket: WebSocket;

function isConnected() {
    return wsSocket && wsSocket.readyState == wsSocket.OPEN;
}

function disconnect() {
    closeVideo();
    if (wsSocket) {
        wsSocket.close();
    }
    uiConnectedState = false;
}


let sendQueue: any[] = [];

// Send message to server. If not connected, queue it.
function wsEmit(event_name: string, data: any)
{
    let raw_msg = JSON.stringify({cmd: event_name, data: data});
    if (isConnected()) {
        logAbbrev("ws_emit(): Sending: " + raw_msg);
        wsSocket.send(raw_msg);
    }
    else {
        console.log("ws_emit(): Disconnected, so queuing: " + raw_msg);
        sendQueue.push(raw_msg);
    }
}

// Infinite loop that sends messages from the queue.
// This only ever sends anything if ws_emit() queues messages due to temporary disconnection.
function sendQueueLoop()
{
    while (sendQueue.length > 0) {
        let raw_msg = sendQueue.shift();
        wsSocket.send(raw_msg);
    }
    setTimeout(sendQueueLoop, 500);
}
setTimeout(sendQueueLoop, 500); // Start the loop


let reconnectDelay = 100;  // for exponential backoff


function connectWebsocket(wsUrl: string) {
    const auth_url = wsUrl.replace(/^wss:/, "https:").replace(/^ws:/, "http:").replace(/\/api\/.*$/, "/api/health");

    function scheduleReconnect() {
        reconnectDelay = Math.round(Math.min(reconnectDelay * 1.5, 5000));
        console.log("API reconnecting in " + reconnectDelay + " ms");
        setTimeout(() => { connectWebsocket(wsUrl); }, reconnectDelay);
        setTimeout(() => { if (!isConnected()) uiConnectedState = false; }, 3000);
    }

    try {
        return fetch(auth_url)
        .then(response => {
            if (response.ok) {
                console.log("Authentication check OK. Connecting to WS API");
                return connectWebsocketAfterAuthCheck(wsUrl);
            } else if (response.status === 401 || response.status === 403) {
                console.log("Auth failed. Status: " + response.status);
                if (reconnectDelay > 1500) {
                    // Force full reload to show login page
                    window.location.reload();
                }
            } else {
                throw new Error(`HTTP auth check ERROR: ${response.status}`);
            }
            scheduleReconnect();
        })
        .catch(error => {
            console.error('HTTP auth check failed:', error);
            scheduleReconnect();
        });
    } catch (error) {
        scheduleReconnect();
    }
}


// Called after we get the API URL from the server.
function connectWebsocketAfterAuthCheck(ws_url: string)
{
    if (!ws_url) throw Error("API URL not specified in config file");

    console.log("...CONNECTING to WS API: " + ws_url);
    wsSocket = new WebSocket(ws_url);


    // Handle connection opening
    wsSocket.addEventListener("open", function (_event) {
        reconnectDelay = 100;
        uiConnectedState = true;

        console.log("Socket connected");
        //acts.add({mode: 'info', message: 'Connected.', lifetime: 1.5});
        if ($videoHash) {
            wsEmit('open_video', {video_hash: $videoHash});
        } else {
            wsEmit('list_my_videos', {});
            wsEmit('list_my_messages', {});
        }
    });

    function handleWithErrors(func: { (): any; }): any {
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
    wsSocket.addEventListener("close", function (_event) {
        reconnectDelay = Math.round(Math.min(reconnectDelay * 1.5, 5000));
        console.log("API reconnecting in " + reconnectDelay + " ms");
        setTimeout(() => { connectWebsocket(ws_url); }, reconnectDelay);
        setTimeout(() => { if (!isConnected()) uiConnectedState = false; }, 3000);
    });

    if (prevCollabId != $collabId) {
        // We have a new collab id. Close old and open new one.
        if (prevCollabId)
        wsEmit('leave_collab', {});
        if ($collabId)
        wsEmit('join_collab', {collab_id: $collabId, video_hash: $videoHash});
    }

    // Incoming messages
    wsSocket.addEventListener("message", function (event)
    {
                                                            console.log("RAW WS MESSAGE: " + event.data);

        const msgJson = JSON.parse(event.data);
        handleWithErrors(() =>
        {
            const cmd = Proto3.ServerToClientCmd.fromJSON(msgJson);
            if (!cmd) {
                console.error("Got INVALID message: ", msgJson);
                return;
            }
            console.log("Got '" + Object.keys(msgJson)[0] + "'");

            if (Date.now() - lastVideoProgressMsgTime > 5000) {
                $videoProgressMsg = null; // timeout progress message after a while
            }

            // welcome
            if (cmd.welcome) {
                if (!cmd.welcome.user) {
                    console.error("No user in welcome message");
                    acts.add({mode: 'danger', message: 'No user in welcome message', lifetime: 5});
                    return;
                }
                $curUsername = cmd.welcome.user.displayname ?? cmd.welcome.user.username;
                $curUserId = cmd.welcome.user.username;
            }
            // error
            else if (cmd.error) {
                console.error("[SERVER ERROR]: ", cmd.error);
                acts.add({mode: 'danger', message: cmd.error.msg, lifetime: 5});
            }
            // showPage
            else if (cmd.showPage) {
                $curPageItems = cmd.showPage.pageItems;
            }
            // defineActions
            else if (cmd.defineActions) {
                $serverDefinedActions = cmd.defineActions.actions;
            }
            // messages
            else if (cmd.showMessages) {
                for (const msg of cmd.showMessages.msgs) {
                    if ( msg.type === Proto3.UserMessage_Type.PROGRESS ) {
                        if (msg.refs?.videoHash == $videoHash) {
                            $videoProgressMsg = msg.message;
                            lastVideoProgressMsgTime = Date.now();
                        }
                    }
                    else if ( msg.type === Proto3.UserMessage_Type.VIDEO_UPDATED ) {
                        refreshMyVideos();
                    } else {
                        $userMessages = $userMessages.filter((m) => m.id != msg.id);
                        if (msg.created) { $userMessages.push(msg); }
                        if (!msg.seen) {
                            const severity = (msg.type == Proto3.UserMessage_Type.ERROR) ? 'danger' : 'info';
                            acts.add({mode: severity, message: msg.message, lifetime: 5});
                            if (severity == 'info') {
                                refreshMyVideos();    // hack, rename and other such actions send info notifications
                            }
                        };
                    }
                }
            }
            // openVideo
            else if (cmd.openVideo) {
                try {
                    const v = cmd.openVideo.video!;
                    if (!v.playbackUrl) throw Error("No playback URL");
                    if (!v.duration) throw Error("No duration");
                    if (!v.title) throw Error("No title");

                    $videoUrl = v.playbackUrl;
                    $videoHash = v.videoHash;
                    $videoFps = parseFloat(v.duration.fps);
                    if (isNaN($videoFps)) throw Error("Invalid FPS");
                    $videoTitle = v.title;
                    $allComments = [];

                    if ($collabId)
                        wsEmit('join_collab', {collab_id: $collabId, video_hash: $videoHash});
                    else
                        history.pushState($videoHash, '', '/?vid=' + $videoHash);  // Point URL to video
                } catch(error) {
                    acts.add({mode: 'danger', message: 'Bad video open request. See log.', lifetime: 5});
                    console.error("Invalid video open request. Error: ", error);
                }
            }
            // addComments
            else if (cmd.addComments) {

                // Add/replace the new comments
                for (const newComment of cmd.addComments.comments) {
                    if (newComment.videoHash != $videoHash) {
                        console.warn("Comment not for current video. Ignoring.");
                        continue;
                    }
                    $allComments = $allComments.filter((c) => c.comment.id !== newComment.id);
                    $allComments.push({
                        comment: newComment,
                        indent: 0
                    });
                }

                // Re-sort / turn updated comment tree into an indented, ordered list for UI
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
                $allComments = indentCommentTree($allComments);
            }
            // delComment
            else if (cmd.delComment) {
                $allComments = $allComments.filter((c) => c.comment.id != cmd.delComment!.commentId);
            }
            // collabEvent
            else if (cmd.collabEvent) {
                const evt = cmd.collabEvent;
                if (!evt.paused) {
                    videoPlayer.collabPlay(evt.seekTimeSec);
                } else {
                    videoPlayer.collabPause(evt.seekTimeSec, evt.drawing);
                }
                if (lastCollabControllingUser != evt.fromUser) {
                    lastCollabControllingUser = evt.fromUser;
                    acts.add({mode: 'info', message: lastCollabControllingUser + " is controlling", lifetime: 5});
                }
            }
            else {
                console.error("[SERVER] UNKNOWN command from server. Raw JSON:", msgJson);
            }
        });
    });
}

function onRequestVideoDelete(videoHash: string, videoName: string) {
    logAbbrev("onRequestVideoDelete: " + videoHash + " / " + videoName);
    wsEmit('del_video', {video_hash: videoHash});
    wsEmit('list_my_videos', {});
}

function onRequestVideoRename(videoHash: string, videoName: string) {
    logAbbrev("onRequestVideoRename: " + videoHash + " / " + videoName);
    let newName = prompt("Rename video to:", videoName);
    if (newName) {
        wsEmit('rename_video', {video_hash: videoHash, new_name: newName});
        wsEmit('list_my_videos', {});
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
    async function call_server(cmd: string, args: Object): Promise<void> { wsEmit(cmd, args); }
    async function call_organizer(cmd: string, args: Object): Promise<void> { wsEmit("organizer", {cmd, args}); }
    async function alert(msg: string): Promise<void> { window.alert(msg); }
    async function prompt(msg: string, default_value: string): Promise<string|null> { return window.prompt(msg, default_value); }
    async function confirm(msg: string): Promise<boolean> { return window.confirm(msg); }

    const AsyncFunction = async function () {}.constructor;
    // @ts-ignore
    let scriptFn = new AsyncFunction("call_server", "call_organizer", "alert", "prompt", "confirm", "items", code);

    console.log("Calling organizer script. Code = ", code, "items=", items);

    scriptFn(call_server, call_organizer, alert, prompt, confirm, items)
    .catch((e: any) => {
        console.error("Error in organizer script:", e);
        acts.add({mode: 'error', message: "Organizer script error. See log.", lifetime: 5});
    });
}

function onVideoListPopupAction(e: { detail: { action: Proto3.ActionDef, items: VideoListDefItem[] }})
{
    let {action, items} = e.detail;
    let itemsObjs = items.map((it) => it.obj);
    console.log("onVideoListPopupAction: ", action, itemsObjs);
    callOrganizerScript(action.action?.code, itemsObjs);
}
</script>


<svelte:window on:popstate={popHistoryState}/>

<main>
    <span id="popup-container"></span>
    <div class="flex flex-col bg-[#101016] w-screen h-screen {debugLayout?'border-2 border-yellow-300':''}">
        <div class="flex-none w-full"><NavBar on:clear-all={onClearAll} on:basic-auth-logout={disconnect} /></div>
        <div class="flex-grow w-full overflow-auto {debugLayout?'border-2 border-cyan-300':''}">
            <Notifications />

        {#if !uiConnectedState }

        <!-- ========== "connecting" spinner ============= -->
        <div transition:fade class="w-full h-full text-5xl text-slate-600 align-middle text-center">
            <h1 class="m-16" style="font-family: 'Yanone Kaffeesatz', sans-serif;">
                Connecting server...
            </h1>
            <div class="fa-2x block">
                <i class="fas fa-spinner connecting-spinner"></i>
            </div>

        </div>

        {:else if $videoHash}

        <!-- ========== video review widgets ============= -->
        <div transition:slide class="flex h-full w-full {debugLayout?'border-2 border-blue-700':''}">

            <div transition:slide class="flex-1 flex flex-col {debugLayout?'border-2 border-purple-600':''}">
                <div class="flex-1 bg-cyan-900">
                    <VideoPlayer
                    bind:this={videoPlayer} src={$videoUrl}
                    on:seeked={onVideoSeeked}
                    on:collabReport={onCollabReport}
                    on:commentPinClicked={onCommentPinClicked}
                    />
                </div>
                <div class="flex-none w-full p-2 {debugLayout?'border-2 border-green-500':''}">
                    <CommentInput bind:this={commentInput} on:button-clicked={onCommentInputButton} />
                </div>
            </div>

            {#if $allComments.length > 0}
            <!-- ========== comment sidepanel ============= -->
            <div id="comment_list" transition:fade class="flex-none w-72 basis-128 bg-gray-900 py-2 px-2 space-y-2 ml-2 overflow-y-auto">
                {#each $allComments as it}
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

        {#if $collabId && !collabDialogAck}
        <div class="fixed top-0 left-0 w-full h-full flex justify-center items-center">
            <div class="bg-gray-900 text-white p-4 rounded-md shadow-lg text-center leading-loose">
                <p class="text-xl text-green-500">Collaborative viewing session active.</p>
                <p class="">Session ID is <code class="text-green-700">{$collabId}</code></p>
                <p class="">Actions like seek, play and draw are mirrored to all participants.</p>
                <p class="">To invite people, copy browser URL and send it to them.</p>
                <p class="">Exit by clicking the green icon in header.</p>
                <button class="bg-gray-800 hover:bg-gray-700 text-green m-2 p-2 rounded-md shadow-lg" on:click|preventDefault="{()=>collabDialogAck=true}">Understood</button>
            </div>
        </div>
        {/if}

        {:else}

        <!-- ========== page components ============= -->
        <div class="organizer_page">
            {#each $curPageItems as item}
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
                <FileUpload postUrl={uploadUrl}>
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
                {#if $userMessages.length>0}
                <h1 class="text-2xl m-6 mt-12 text-slate-500">
                    Latest messages
                </h1>
                <div class="gap-4 max-h-56 overflow-y-auto border-l px-2 border-gray-900">
                    {#each $userMessages as msg}
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
