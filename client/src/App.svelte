<script lang="ts">
import {Notifications, acts} from '@tadashi/svelte-notification'
import {fade, slide} from "svelte/transition";

import * as Proto3 from '@clapshot_protobuf/typescript';

import {allComments, curUsername, curUserId, videoIsReady, mediaFileId, curVideo, curPageId, curPageItems, userMessages, latestProgressReports, collabId, userMenuItems, serverDefinedActions, curUserIsAdmin, connectionErrors, curSubtitle} from './stores';
import {IndentedComment, type UserMenuItem, type StringMap, type MediaProgressReport} from "./types";

import CommentCard from './lib/player_view/CommentCard.svelte'
import SubtitleCard from './lib/player_view/SubtitleCard.svelte';
import NavBar from './lib/NavBar.svelte'
import CommentInput from './lib/player_view/CommentInput.svelte';
import UserMessage from './lib/UserMessage.svelte';
import FileUpload from './lib/asset_browser/FileUpload.svelte';
import VideoPlayer from './lib/player_view/VideoPlayer.svelte';
import {folderItemsToIDs, type VideoListDefItem} from "@/lib/asset_browser/types";
import FolderListing from './lib/asset_browser/FolderListing.svelte';
import LocalStorageCookies from './cookies';
import RawHtmlItem from './lib/asset_browser/RawHtmlItem.svelte';
import { ClientToServerCmd } from '@clapshot_protobuf/typescript/dist/src/client';

let videoPlayer: VideoPlayer;
let commentInput: CommentInput;
let debugLayout: boolean = false;
let uiConnectedState: boolean = false; // true if UI should look like we're connected to the server

let collabDialogAck = false;  // true if user has clicked "OK" on the collab dialog
let lastCollabControllingUser: string | null = null;    // last user to control the video in a collab session

let wsSocket: WebSocket | undefined;
let sendQueue: any[] = [];


function logAbbrev(...strs: any[]) {
    /*
    const maxLen = 180;
    let abbreviated: string[] = [];
    for (let i = 0; i < strs.length; i++) {
        let str = (typeof strs[i] == "string" || typeof strs[i] == "number" || typeof strs[i] == "boolean")
        ? String(strs[i])
        : JSON.stringify(strs[i]);
        abbreviated[i] = (str.length > maxLen) ? (str.slice(0, maxLen) + " ……") : str;
    }
    console.log(...abbreviated);
    */
    console.log(...strs);
}

// Log JSON object with console.dir, optionally wrapped in a message (op_name)
// If obj is a string, try to parse it as JSON first, then log it. If it fails, log the string as-is.
function richLog(obj: any, op_name: string|undefined = undefined, proto3_cmd: any = undefined) {

    let cmd_name = "";
    if (proto3_cmd) {
        const first_non_nullish_key = (obj: any) => Object.keys(obj).find(key => (obj[key] !== null && obj[key] !== undefined));
        cmd_name = first_non_nullish_key(proto3_cmd) ?? "(unknown cmd)";
    }

    let parsed = null;
    try { parsed = JSON.parse(obj); } catch (e) { parsed = obj; }

    if (op_name || cmd_name) {
        let prefix = (op_name ? ("["+op_name+"]") : "") + (cmd_name ? (" " + cmd_name) : "");
        console.log(prefix, parsed);
    }
    else console.log(parsed);
}


// Show last 5 connection errors and log everything to console
function showConnectionError(msg: string) {
    connectionErrors.update((errs: string[]) => {
        let t = new Date().toLocaleTimeString();
        errs.push(`[${t}] ${msg}`);
        return errs.slice(-10);
    });
    console.error("[CONNECTION ERROR]", msg);
}

// Messages from CommentInput component
function onCommentInputButton(e: any) {

    const PLAYBACK_REQ_SOURCE = "comment_input";
    function resumePlayer() {
        // Only resume if playback was paused by comment input
        if (videoPlayer.getPlaybackState().request_source == PLAYBACK_REQ_SOURCE) {
            videoPlayer.setPlayback(true, PLAYBACK_REQ_SOURCE);
        }
    }
    function pausePlayer() {
        videoPlayer.setPlayback(false, PLAYBACK_REQ_SOURCE);
    }

    if (e.detail.action == "send")
    {
        if (e.detail.comment_text != "")
        {
            wsEmit({addComment: {
                mediaFileId: $mediaFileId!,
                comment: e.detail.comment_text,
                drawing: videoPlayer.getScreenshot(),
                timecode: e.detail.is_timed ? videoPlayer.getCurTimecode() : "",
                subtitleId: $curSubtitle?.id
            }});
        }
        resumePlayer();
    }
    else if (e.detail.action == "text_input") {
        pausePlayer();   // auto-pause when typing a comment
    }
    else if (e.detail.action == "color_select") {
        pausePlayer();
        videoPlayer.onColorSelect(e.detail.color);
    }
    else if (e.detail.action == "draw") {
        if (e.detail.is_draw_mode) { pausePlayer(); }
        videoPlayer.onToggleDraw(e.detail.is_draw_mode);
    }
    else if (e.detail.action == "undo") {
        pausePlayer();
        videoPlayer.onDrawUndo();
    }
    else if (e.detail.action == "redo") {
        pausePlayer();
        videoPlayer.onDrawRedo();
    }
}

function onDisplayComment(e: any) {
    if (!$curVideo) { throw Error("No video loaded"); }
    videoPlayer.seekToSMPTE(e.detail.timecode);
    // Close draw mode while showing (drawing from a saved) comment
    videoPlayer.onToggleDraw(false);
    commentInput.forceDrawMode(false);
    if (e.detail.drawing) { videoPlayer.setDrawing(e.detail.drawing); }
    if (e.detail.subtitleId) { $curSubtitle = $curVideo.subtitles.find((s) => s.id == e.detail.subtitleId) ?? null; }
    if ($collabId) {
        logAbbrev("Collab: onDisplayComment. collab_id: '" + $collabId + "'");
        wsEmit({collabReport: {
            paused: true,
            loop: videoPlayer.isLooping(),
            seekTimeSec: videoPlayer.getCurTime(),
            drawing: e.detail.drawing,
            subtitleId: $curSubtitle?.id
        }});
    }
}

function onDeleteComment(e: any) {
    wsEmit({delComment: { commentId: e.detail.id }});
}

function onReplyComment(e: { detail: { parentId: string; commentText: string, subtitleId: string|undefined }}) {
    console.log("onReplyComment: ", e.detail);
    wsEmit({addComment: {
        mediaFileId: $mediaFileId!,
        parentId: e.detail.parentId,
        comment: e.detail.commentText,
        subtitleId: e.detail.subtitleId,
    }});
}

function onEditComment(e: any) {
    wsEmit({editComment: {
        commentId: e.detail.id,
        newComment: e.detail.comment_text,
    }});
}

function closePlayerIfOpen() {
    console.debug("closePlayerIfOpen()");
    wsEmit({leaveCollab: {}});
    $collabId = null;
    $mediaFileId = null;
    $curVideo = null;
    $allComments = [];
    $videoIsReady = false;
}

function onPlayerSeeked(_e: any) {
    commentInput.forceDrawMode(false);  // Close draw mode when video frame is changed
}

function onCollabReport(e: { detail: { report: Proto3.client.ClientToServerCmd_CollabReport; }; }) {
    if ($collabId) {
        wsEmit({collabReport: e.detail.report});
    }
}

function onCommentPinClicked(e: any) {
    // Find corresponding comment in the list, scroll to it and highlight
    let commentId = e.detail.id;
    let c = $allComments.find((c: { comment: { id: any; }; }) => c.comment.id == commentId);
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

function onSubtitleChange(e: any) {
    const sub_id = e.detail.id;
    if (!$curVideo) { throw Error("No video loaded"); }
    console.debug("onSubtitleChange, id:", sub_id, "allSubtitles:", $curVideo.subtitles);
    if ($curSubtitle?.id == sub_id) {
        $curSubtitle = null;
    } else {
        $curSubtitle = $curVideo.subtitles.find((s) => s.id == sub_id) ?? null;
        if ($curSubtitle == null && sub_id != null) {
            console.error("Subtitle not found: ", sub_id);
            acts.add({mode: 'error', message: "Subtitle not found. See log.", lifetime: 5});
        }
    }
    if ($collabId) {
        wsEmit({collabReport: {
            paused: videoPlayer.isPaused(),
            loop: videoPlayer.isLooping(),
            seekTimeSec: videoPlayer.getCurTime(),
            drawing: videoPlayer.getScreenshot(),
            subtitleId: $curSubtitle?.id,
        }});
    }
    console.debug("Subtitle URL changed to: ", $curSubtitle?.playbackUrl);
}

// User clicked on subtitle upload icon
async function onUploadSubtitles() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.srt, .vtt, .ssa, .ass';
    input.click();

    input.onchange = async () => {
        if (!input.files?.length) {
            console.log('No subtitle file selected. Skipping upload.');
            return;
        }
        for (let file of input.files) {
            const reader = new FileReader();
            reader.onload = function(event) {
                try {
                    if (!event.target?.result) { throw new Error('No file contents read'); }
                    const dataUrl = event.target.result as string;
                    const [_, contentsBase64] = dataUrl.split(',');
                    wsEmit({ addSubtitle : {
                        mediaFileId: $mediaFileId!,
                        fileName: file.name,
                        contentsBase64
                    }});
                } catch (e) {
                    console.error('Error adding subtitle file:', e);
                    acts.add({mode: 'error', message: 'Error adding subtitle file. See log.', lifetime: 5});
                }
            };
            reader.readAsDataURL(file);
        }
    };
}

function onSubtitleDelete(e: any) {
    const sub_id = e.detail.id;
    if (window.confirm("Are you sure you want to delete this subtitle?")) {
        if ($curSubtitle?.id == sub_id) { $curSubtitle = null; }
        wsEmit({ delSubtitle: { id: sub_id } });
    }
}

/*
export interface ClientToServerCmd_EditSubtitleInfo {
    id: string;
    title?: string | undefined;
    languageCode?: string | undefined;
    timeOffset?: number | undefined;
    _unknownFields?: {
        [key: number]: Uint8Array[];
    } | undefined;
}
*/

async function onSubtitleUpdate(e: any) {
    const sub = e.detail.sub;
    const isDefault = e.detail.isDefault;
    if (isNaN(sub.timeOffset)) {
        console.error("Invalid time offset: ", sub.timeOffset);
        acts.add({mode: 'error', message: "Invalid time offset: " + sub.timeOffset, lifetime: 5});
        return;
    }
    wsEmit({ editSubtitleInfo: {
        id: sub.id,
        title: sub.title,
        languageCode: sub.languageCode,
        timeOffset: sub.timeOffset,
        isDefault,
    }});
}

function popHistoryState(e: PopStateEvent) {
    console.debug("popHistoryState called. e.state=", e.state);
    if (e.state) {
        if (e.state.mediaFileId) {
            console.debug("popHistoryState: Opening video: ", e.state.mediaFileId);
            wsEmit({ openMediaFile: { mediaFileId: e.state.mediaFileId } });
            return;
        } else if (e.state.pageId) {
            console.debug("popHistoryState: Opening page: ", e.state.pageId);
            wsEmit({openNavigationPage: {pageId: e.state.pageId ?? undefined}});
            return;
        }
    }
    console.debug("popHistoryState: Resetting UI view due to empty state");
    closePlayerIfOpen();
    wsEmit({openNavigationPage: {pageId: undefined}});
}

// On full page load, parse URL parameters to see if we have a
// video or page ID to open directly.
const prevCollabId = $collabId;

const urlParams = new URLSearchParams(window.location.search);
urlParams.forEach((value, key) => {
    if (key != "vid" && key != "collab" && key != "p") {
        console.error("Got UNKNOWN URL parameter: '" + key + "'. Value= " + value);
        acts.add({mode: 'warn', message: "Unknown URL parameter: '" + key + "'", lifetime: 5});
    }
});

console.debug("Parsing URL params: ", urlParams);

$mediaFileId = urlParams.get('vid');
$collabId = urlParams.get('collab');

const encodedPageParm = urlParams.get('p');
$curPageId = encodedPageParm ? decodeURIComponent(encodedPageParm) : null;

if ($mediaFileId && $collabId)
    history.replaceState({mediaFileId: $mediaFileId}, '', `/?vid=${$mediaFileId}&collab=${$collabId}`);
else if ($mediaFileId)
    history.replaceState({mediaFileId: $mediaFileId}, '', `/?vid=${$mediaFileId}`);
else if ($curPageId)
    history.replaceState({pageId: $curPageId}, '', `/?p=${encodeURIComponent($curPageId)}`);
else
    history.replaceState({}, '', './');


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
    console.log("Config file '" + CONF_FILE + "' parsed: ", json);
    uploadUrl = json.upload_url;

    console.log("Connecting to WS API at: " + json.ws_url);
    connectWebsocket(json.ws_url);

    $userMenuItems = json.user_menu_extra_items;
    if (json.user_menu_show_basic_auth_logout) {
        $userMenuItems = [...$userMenuItems, {label: "Logout", type: "logout-basic-auth"} as UserMenuItem];
    }
    $userMenuItems = [...$userMenuItems, {label: "About", type: "about"} as UserMenuItem];
})
.catch(error => {
    showConnectionError(`Failed to read config file '${CONF_FILE}': ${error}`);
});


let videoListRefreshScheduled = false;
function refreshMyMediaFiles()
{
    if (!videoListRefreshScheduled) {
        videoListRefreshScheduled = true;
        setTimeout(() => {
            videoListRefreshScheduled = false;
            if (!$mediaFileId) {
                console.debug("refreshMyMediaFiles timer fired, no mediaFileId. Requesting openNavigationPage.");
                wsEmit({openNavigationPage: {pageId: $curPageId ?? undefined}});
            } else {
                console.debug("refreshMyMediaFiles timer fired, mediaFileId present. Ignoring.");
            }
        }, 500);
    }
}


function isConnected() {
    return wsSocket && wsSocket.readyState == wsSocket.OPEN;
}

function disconnect() {
    closePlayerIfOpen();
    $curPageId = null;
    if (wsSocket) {
        wsSocket.close();
    }
    uiConnectedState = false;
}


// Send message to server. If not connected, queue it.
function wsEmit(cmd: ClientToServerCmd)
{
    let cookies = LocalStorageCookies.getAllNonExpired();
    let raw_msg = JSON.stringify({ ...cmd, cookies });

    if (isConnected()) {
        richLog(raw_msg, "SEND", cmd);
        wsSocket?.send(raw_msg);
    }
    else {
        richLog(raw_msg, "SEND (disconnected, so queuing)", cmd);
        sendQueue.push(raw_msg);
    }
}


// Infinite loop that sends messages from the queue.
// This only ever sends anything if ws_emit() queues messages due to temporary disconnection.
function sendQueueLoop()
{
    while (wsSocket && sendQueue.length > 0) {
        let raw_msg = sendQueue.shift();
        wsSocket.send(raw_msg);
    }
    setTimeout(sendQueueLoop, 500);
}
setTimeout(sendQueueLoop, 500); // Start the loop


let reconnectDelay = 100;  // for exponential backoff


function connectWebsocket(wsUrl: string) {
    const http_health_url = wsUrl.replace(/^wss:/, "https:").replace(/^ws:/, "http:").replace(/\/api\/.*$/, "/api/health");
    let req_init: RequestInit = {
        method: 'GET',
        headers: {
            'Content-Type': 'application/json',
            'Accept': 'application/json',
            'X-Clapshot-Cookies': JSON.stringify(LocalStorageCookies.getAllNonExpired()),
        },
    };

    function scheduleReconnect() {
        reconnectDelay = Math.round(Math.min(reconnectDelay * 1.5, 5000));
        console.log("API reconnecting in " + reconnectDelay + " ms");
        setTimeout(() => { connectWebsocket(wsUrl); }, reconnectDelay);
        setTimeout(() => { if (!isConnected()) uiConnectedState = false; }, 3000);
    }

    try {
        return fetch(http_health_url, req_init)
        .then(response => {
            if (response.ok) {
                console.log("Authentication check OK. Connecting to WS API");
                return connectWebsocketAfterAuthCheck(wsUrl);
            } else {
                if (response.status === 401 || response.status === 403) {
                    if (reconnectDelay > 1500)    // don't reload too often, just retry the fetch
                        window.location.reload();
                    else
                        scheduleReconnect();
                }
                showConnectionError(`Auth error at '${http_health_url}': ${response.status} - ${response.statusText}`);
            }
        })
        .catch(error => {
            if (error.name === 'TypeError' && error.message == 'Failed to fetch') {
            showConnectionError(`Network error fetching '${http_health_url}'`);
            } else {
                showConnectionError(`Failed to fetch '${http_health_url}': ${error}`);
            }
            scheduleReconnect();
        });
    } catch (error) {
        showConnectionError(`Connect to '${wsUrl}' failed: ${error}`);
        scheduleReconnect();
    }
}


// Sets a temporary progress bar / message for a media file.
// This is used to show transcoding progress.
function addMediaProgressReport(mediaFileId?: String, msg?: String, progress?: number)
{
    let report = { mediaFileId, msg, progress, received_ts: Date.now() } as MediaProgressReport

    // Filter out any old reports for this media file
    $latestProgressReports = $latestProgressReports.filter((r: MediaProgressReport) => r.mediaFileId != report.mediaFileId);
    if (report.progress !== 1.0) {  // Hide progress bar immediately when done
        $latestProgressReports = [...$latestProgressReports, report];
    }

    // Schedule a cleanup of expired reports
    setTimeout(() => {
        $latestProgressReports = $latestProgressReports.filter((r: MediaProgressReport) => r.received_ts > (Date.now() - 6000));
        $latestProgressReports = [...$latestProgressReports];   // force svelte to re-render
    }, 1000);
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
        connectionErrors.set([]);

        if ($mediaFileId) {
            console.debug(`Socket connected, mediaFileId=${mediaFileId}. Requesting openMediaFile`);
            wsEmit({openMediaFile: { mediaFileId: $mediaFileId }});
        } else {
            console.debug("Socket connected, no mediaFileId. Requesting openNavigationPage");
            wsEmit({openNavigationPage: {pageId: $curPageId ?? undefined}});
            wsEmit({listMyMessages: {}});
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
            wsEmit({leaveCollab: {}});
        if ($collabId)
            wsEmit({joinCollab: { collabId: $collabId, mediaFileId: $mediaFileId! }});
    }

    // Incoming messages
    wsSocket.addEventListener("message", function (event)
    {
        let cmd = null;
        try {
            const msgJson = JSON.parse(event.data);
            cmd = Proto3.client.ServerToClientCmd.fromJSON(JSON.parse(event.data));
            if (!cmd) {
                console.error("Got INVALID message: ", msgJson);
                return;
            }
        } catch (e) {
            console.error("Error parsing incoming message: ", e);
            console.error("Message data: ", event.data);
            return;
        }

        richLog(event.data, "RECV", cmd);
        handleWithErrors(() =>
        {
            // welcome
            if (cmd.welcome) {
                if (!cmd.welcome.hasOwnProperty("serverVersion") || (process.env.CLAPSHOT_MIN_SERVER_VERSION && cmd.welcome.serverVersion < process.env.CLAPSHOT_MIN_SERVER_VERSION)) {
                    const msg = "Server version too old (v" + cmd.welcome.serverVersion + "). Please update server.";
                    console.error(msg);
                    window.alert(msg);
                    return;
                }
                console.log("Connected to server v" + cmd.welcome.serverVersion);
                if (process.env.CLAPSHOT_MAX_SERVER_VERSION && cmd.welcome.serverVersion > process.env.CLAPSHOT_MAX_SERVER_VERSION) {
                    const msg = "Client version too old (client v" + process.env.CLAPSHOT_CLIENT_VERSION + " for server v" + cmd.welcome.serverVersion + "). Please update client.";
                    console.error(msg);
                    window.alert(msg);
                    return;
                }

                if (!cmd.welcome.user) {
                    console.error("No user in welcome message");
                    acts.add({mode: 'danger', message: 'No user in welcome message', lifetime: 5});
                    return;
                }
                $curUsername = cmd.welcome.user.name ?? cmd.welcome.user.id;
                $curUserId = cmd.welcome.user.id;
                $curUserIsAdmin = cmd.welcome.isAdmin;
            }
            // error
            else if (cmd.error) {
                console.error("[SERVER ERROR]: ", cmd.error);
                acts.add({mode: 'danger', message: cmd.error.msg, lifetime: 5});
            }
            // showPage
            else if (cmd.showPage) {
                const newPageId = cmd.showPage.pageId ?? null;  // turn undefined into null
                console.debug("showPage. newPageId=", newPageId, "$curPageId=", $curPageId);

                // Record page ID in browser history
                if (newPageId !== $curPageId) {   // Changed id looks like a new page to user
                    if (newPageId !== null) {
                        console.debug("[Browser history] Pushing new page state: ", newPageId);
                        history.pushState({pageId: newPageId}, '', `/?p=${encodeURIComponent(newPageId)}`);
                        document.title = "Clapshot - " + (cmd.showPage.pageTitle ?? newPageId);
                    } else {
                        console.debug("[Browser history] Pushing empty state (default page)");
                        history.pushState({pageId: null}, '', './');
                        document.title = "Clapshot - Home";
                    }
                }

                $curPageId = newPageId;
                closePlayerIfOpen();  // No-op if no video is open
                $curPageItems = [...cmd.showPage.pageItems];  // force svelte to re-render
            }
            // defineActions
            else if (cmd.defineActions) {
                for (var name in cmd.defineActions.actions)
                    $serverDefinedActions[name] = cmd.defineActions.actions[name];
            }
            // messages
            else if (cmd.showMessages) {
                for (const msg of cmd.showMessages.msgs) {
                    if ( msg.type === Proto3.UserMessage_Type.PROGRESS ) {
                        addMediaProgressReport(msg.refs?.mediaFileId, msg.message, msg.progress);
                    }
                    else if ( msg.type === Proto3.UserMessage_Type.MEDIA_FILE_UPDATED ) {
                        refreshMyMediaFiles();
                    }
                    else if ( msg.type === Proto3.UserMessage_Type.MEDIA_FILE_ADDED ) {
                        console.log("Handling MEDIA_FILE_ADDED: ", msg);
                        if (!msg.refs?.mediaFileId) { console.error("MEDIA_FILE_ADDED message with no mediaFileId. This is a bug."); }

                        // Parse details and extract JSON data (added by FileUpload) from msg
                        const uploadCookies = JSON.parse(msg.details ?? '{}');
                        const listingData = JSON.parse(uploadCookies.listing_data_json ?? '{}');
                        const addedAction = uploadCookies.media_file_added_action;

                        // Call organizer script if defined, otherwise refresh video list
                        if (addedAction) {
                            const action = $serverDefinedActions[addedAction];
                            if (!action) {
                                const errorMsg = `Undefined media_file_added_action: '${addedAction}'`;
                                acts.add({ mode: 'error', message: errorMsg, lifetime: 5 });
                                console.error(errorMsg);
                            } else {
                                callOrganizerScript(action.action, {
                                    media_file_id: msg.refs?.mediaFileId,
                                    listing_data: listingData,
                                });
                            }
                        } else {
                            refreshMyMediaFiles();
                        }
                    }
                    else {
                        $userMessages = $userMessages.filter((m) => m.id != msg.id);
                        if (msg.created) { $userMessages.push(msg); }
                        if (!msg.seen) {
                            const severity = (msg.type == Proto3.UserMessage_Type.ERROR) ? 'danger' : 'info';
                            let fileinfo = msg.refs?.mediaFileId ? (msg.refs.mediaFileId + " – ") : "";
                            acts.add({mode: severity, message: fileinfo + msg.message, lifetime: 5});
                            if (severity == 'info') {
                                refreshMyMediaFiles();    // hack, rename and other such actions send info notifications
                            }
                        };
                        $userMessages = [...$userMessages];  // force svelte to re-render

                        // Some "normal" messages can also set progress bars (e.g. "Transcoding done")
                        if (msg.progress !== undefined && msg.refs?.mediaFileId) {
                            addMediaProgressReport(msg.refs?.mediaFileId, msg.message, msg.progress);
                        }
                    }
                }
            }
            // openMediaFile
            else if (cmd.openMediaFile) {
                try {
                    const v: Proto3.MediaFile = cmd.openMediaFile.mediaFile!;
                    if (!v.playbackUrl) throw Error("No playback URL");
                    if (!v.duration) throw Error("No duration");
                    if (!v.title) throw Error("No title");

                    $curPageId = null;  // Clear the current page ID, so popHistoryState will know to reopen it if needed

                    if ($mediaFileId != v.id) {
                        console.debug("[Browser history] Pushing new media file state: ", v.id);
                        history.pushState({mediaFileId: v.id}, '', `/?vid=${v.id}`);
                        document.title = "Clapshot - " + (v.title ?? v.id);
                    }

                    $mediaFileId = v.id;
                    $curVideo = v;
                    $allComments = [];

                    if (v.defaultSubtitleId) {
                        $curSubtitle = $curVideo.subtitles.find((s) => s.id == v.defaultSubtitleId) ?? null;
                    } else {
                        let old_id = $curSubtitle?.id;
                        $curSubtitle = $curVideo.subtitles.find((s) => s.id == old_id) ?? null;
                    }

                    if ($collabId)
                        wsEmit({joinCollab: { collabId: $collabId, mediaFileId: $mediaFileId! }});

                } catch(error) {
                    acts.add({mode: 'danger', message: 'Bad video open request. See log.', lifetime: 5});
                    console.error("Invalid video open request. Error: ", error);
                }
            }
            // addComments
            else if (cmd.addComments) {

                // Add/replace the new comments
                for (const newComment of cmd.addComments.comments) {
                    if (newComment.mediaFileId != $mediaFileId) {
                        console.warn("Comment not for current video. Ignoring.");
                        continue;
                    }
                    $allComments = $allComments.filter((c: { comment: { id: string; }; }) => c.comment.id !== newComment.id);
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
                $allComments = $allComments.filter((c: { comment: { id: string; }; }) => c.comment.id != cmd.delComment!.commentId);
            }
            // collabEvent
            else if (cmd.collabEvent) {
                const evt = cmd.collabEvent;
                if (evt.subtitleId != $curSubtitle?.id) {
                    $curSubtitle = $curVideo?.subtitles.find((s) => s.id == evt.subtitleId) ?? null;
                }
                if (!evt.paused) {
                    videoPlayer.collabPlay(evt.seekTimeSec, evt.loop);
                } else {
                    videoPlayer.collabPause(evt.seekTimeSec, evt.loop, evt.drawing);
                }
                if (lastCollabControllingUser != evt.fromUser) {
                    lastCollabControllingUser = evt.fromUser;
                    acts.add({mode: 'info', message: lastCollabControllingUser + " is controlling", lifetime: 5});
                }
            }
            // setCookies
            else if (cmd.setCookies) {
                let cookie_dict = cmd.setCookies.cookies;
                if (!cookie_dict) {
                    console.error("[SERVER] setCookies command with no cookies. Cmd:", cmd);
                } else {
                    let expireTimestamp = cmd.setCookies.expireTime?.getTime() ?? null;
                    for (const [key, value] of Object.entries(cookie_dict)) {
                        LocalStorageCookies.set(key, value, expireTimestamp);
                    }
                }
            }
            else {
                console.error("[SERVER] UNKNOWN command from server:", cmd);
            }
        });
    });
}

function onMoveItemsToFolder(e: {detail: {dstFolderId: string; ids: Proto3.FolderItemID[], listingData: StringMap}}) {
    let {dstFolderId, ids, listingData} = e.detail;
    wsEmit({moveToFolder: { dstFolderId, ids, listingData }});
}

function onReorderItems(e: {detail: {ids: Proto3.FolderItemID[], listingData: StringMap}}) {
    let {ids, listingData} = e.detail;
    wsEmit({reorderItems: { listingData, ids }});
}

function openMediaFileListItem(e: { detail: { item: Proto3.PageItem_FolderListing_Item, listingData: StringMap }}): void {
    let {item, listingData} = e.detail;
    if (item.openAction) {
        callOrganizerScript(item.openAction, {
            listing_data: listingData,
            item_to_open: item
        });
    } else {
        console.error("No openAction script for item: " + item);
        acts.add({mode: 'error', message: "No open action for item. See log.", lifetime: 5});
    }
}

// ------------

// Expose some API functions to browser JS (=scripts from Server and Organizer)

(window as any).clapshot = {
    openMediaFile: (mediaFileId: string) => { wsEmit({ openMediaFile: { mediaFileId } }) },
    renameMediaFile: (mediaFileId: string, newName: string) => { wsEmit({ renameMediaFile: { mediaFileId, newName } }) },
    delMediaFile: (mediaFileId: string) => { wsEmit({ delMediaFile: { mediaFileId } }) },

    callOrganizer: (cmd: string, args: Object) => { wsEmit({ organizerCmd: { cmd, args: JSON.stringify(args) } }) },
    itemsToIDs: (items: Proto3.PageItem_FolderListing_Item[]): Proto3.FolderItemID[] => { return folderItemsToIDs(items) },
    moveToFolder: (
        dstFolderId: string,
        ids: Proto3.FolderItemID[],
        listingData: StringMap) => { wsEmit({ moveToFolder: { dstFolderId, ids, listingData } }) },
    reorderItems: (
        ids: Proto3.FolderItemID[],
        listingData: StringMap) => { wsEmit({ reorderItems: { ids, listingData } }) },
};

/// Evalute a string as Javascript from Organizer (or Server)
function callOrganizerScript(script: Proto3.ScriptCall|undefined, action_args: Object): void {
    if (!script || !script.code ) {
        console.log("callOrganizerScript called with empty code. Ignoring.");
        return;
    }
    if (script.lang != Proto3.ScriptCall_Lang.JAVASCRIPT ) {
        console.error("BUG: Unsupported Organizer script language: " + script.lang);
        acts.add({mode: 'error', message: "BUG: Unsupported script lang. See log.", lifetime: 5});
        return;
    }
    const Function = function () {}.constructor;
    // @ts-ignore
    let scriptFn = new Function("_action_args", script.code);
    console.log("Calling organizer script:", {action_args, code: script.code});
    try {
        scriptFn(action_args);
    } catch (e: any) {
        console.error("Error in organizer script:", e);
        acts.add({mode: 'error', message: "Organizer script error. See log.", lifetime: 5});
    }
}

function onMediaFileListPopupAction(e: { detail: { action: Proto3.ActionDef, items: VideoListDefItem[], listingData: StringMap }})
{
    let {action, items, listingData} = e.detail;
    let itemsObjs = items.map((it) => it.obj);
    console.log("onMediaFileListPopupAction():", {action, itemsObjs, listingData});
    callOrganizerScript(action.action, {
                listing_data: listingData,
                selected_items: itemsObjs
            });
}
</script>


<svelte:window on:popstate={popHistoryState}/>

<main>
    <span id="popup-container"></span>
    <div class="flex flex-col bg-[#101016] w-screen h-screen {debugLayout?'border-2 border-yellow-300':''}">
        <div class="flex-none w-full"><NavBar on:basic-auth-logout={disconnect} /></div>
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
            <div class="m-16 text-xs">
                {#if $connectionErrors.length > 0}
                    <details class="connection-errors">
                        <summary class="connection-errors cursor-pointer text-slate-600">View connection errors</summary>
                        <ul>
                            {#each $connectionErrors as ce}
                            <li><code>{ce}</code></li>
                            {/each}
                        </ul>
                        <p class="m-4 text-sm"><em>See browser JS console for more details.</em></p>
                    </details>
                {/if}
            </div>
        </div>

        {:else if $mediaFileId && $curVideo && $curVideo.playbackUrl}

        <!-- ========== video review widgets ============= -->
        <div transition:slide class="flex h-full w-full {debugLayout?'border-2 border-blue-700':''}">

            <div transition:slide class="flex-1 flex flex-col {debugLayout?'border-2 border-purple-600':''}">
                <div class="flex-1 bg-cyan-900">
                    <VideoPlayer
                        bind:this={videoPlayer} src={$curVideo.playbackUrl}
                        on:seeked={onPlayerSeeked}
                        on:collabReport={onCollabReport}
                        on:commentPinClicked={onCommentPinClicked}
                        on:uploadSubtitles={onUploadSubtitles}
                        on:change-subtitle={onSubtitleChange}
                    />
                </div>
                <div class="flex-none w-full p-2 {debugLayout?'border-2 border-green-500':''}">
                    <CommentInput bind:this={commentInput} on:button-clicked={onCommentInputButton} />
                </div>
            </div>

            {#if $allComments.length > 0 || $curSubtitle}
            <div id="comment_list" transition:fade class="flex flex-col h-full w-72 bg-gray-900 py-2 px-2 ml-2">
                <div class="flex-grow overflow-y-auto space-y-2">
                    {#each $allComments as it}
                        <CommentCard
                            indent={it.indent}
                            comment={it.comment}
                            on:display-comment={onDisplayComment}
                            on:delete-comment={onDeleteComment}
                            on:reply-to-comment={onReplyComment}
                            on:edit-comment={onEditComment}
                        />
                    {/each}
                </div>
                <div class="flex-none">
                    {#if $curVideo.subtitles}
                        <!-- Subtitles -->
                        <div class="flex justify-between text-gray-500 items-center py-2 border-t border-gray-500">
                            <h6>Subtitles</h6>
                            <button class="fa fa-plus-circle" title="Upload subtitles" on:click={onUploadSubtitles}></button>
                        </div>
                        {#each $curVideo.subtitles as sub}
                            <SubtitleCard
                                sub={sub}
                                isDefault={$curVideo.defaultSubtitleId == sub.id}
                                on:change-subtitle={onSubtitleChange}
                                on:delete-subtitle={onSubtitleDelete}
                                on:update-subtitle={onSubtitleUpdate}
                            />
                        {/each}
                    {/if}
                </div>
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
                {#each $curPageItems as pit}
                    {#if pit.html }
                        <div class="text-slate-500">
                            <RawHtmlItem html={pit.html} />
                        </div>
                    {:else if pit.folderListing}
                        <div class="my-6">
                            <!-- ========== upload widget ============= -->
                            {#if pit.folderListing.allowUpload }
                                <div class="h-24 border-4 border-dashed border-gray-700">
                                    <FileUpload
                                        postUrl={uploadUrl}
                                        listingData={pit.folderListing.listingData ?? {}}
                                        mediaFileAddedAction={pit.folderListing.mediaFileAddedAction}
                                    >
                                        <div class="flex flex-col justify-center items-center h-full">
                                            <div class="text-2xl text-gray-700">
                                                <i class="fas fa-upload"></i>
                                            </div>
                                            <div class="text-xl text-gray-700">
                                                Drop video, audio and image files here to upload
                                            </div>
                                        </div>
                                    </FileUpload>
                                </div>
                            {/if}
                            <!-- ========== folder widge ============= -->
                            <div class="my-6">
                                <FolderListing
                                    listingData={pit.folderListing.listingData}
                                    items={pit.folderListing.items.map((it)=>({
                                        id: (it.mediaFile?.id ?? it.folder?.id ?? "[BUG: BAD ITEM TYPE]"),
                                        obj: it }))}
                                    dragDisabled = {pit.folderListing.allowReordering ? false : true}
                                    listPopupActions = {pit.folderListing.popupActions}
                                    on:open-item = {openMediaFileListItem}
                                    on:reorder-items = {onReorderItems}
                                    on:move-to-folder = {onMoveItemsToFolder}
                                    on:popup-action = {onMediaFileListPopupAction}
                                />
                            </div>
                        </div>
                    {/if}
                {/each}
            </div>


            <div>
                {#if $userMessages.length>0}
                <h1 class="text-2xl m-6 mt-12 text-slate-500">
                    Latest messages
                </h1>
                <div class="gap-4 max-h-56 overflow-y-auto border-l px-2 border-gray-900" role="log">
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

    /* Remove ugly <video><track> background from captions and make font slightly smaller than default */
    :global(::cue) {
        background: transparent;
        font-size: 90%;
    }

    /* Make all headings in organizer page bigger */
    :global(div.organizer_page){
        margin: 2em;
    }

    :global(.organizer_page h1){
        font-size: 2.5rem;
        line-height: 3rem;
        font-weight: 700;
    }

    :global(.organizer_page h2){
        font-size: 2.5rem;
        line-height: 3rem;
    }

    :global(.organizer_page h3){
        font-size: 1.5rem;
        line-height: 2rem;
    }

    summary.connection-errors {
        padding: 1rem;
        color: #323946;
        font-size: 1rem;
    }
    details[open] summary.connection-errors {
        color: #323946;
        margin-bottom: 1rem;
        border-bottom: 1px solid #323946;
    }
    details[open] {
        margin-bottom: 1rem;
        border-bottom: 1px solid #323946;
    }

</style>
