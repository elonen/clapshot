<script lang="ts">

import {acts} from '@tadashi/svelte-notification'
import {create as sdb_create} from "simple-drawing-board";
import {onMount, createEventDispatcher} from 'svelte';
import {scale} from "svelte/transition";
import '@fortawesome/fontawesome-free/css/all.min.css';
import * as Proto3 from '@clapshot_protobuf/typescript';
import {VideoFrame} from './VideoFrame';
import {allComments, curSubtitle, videoIsReady, videoFps, collabId, allSubtitles} from '@/stores';

import CommentTimelinePin from './CommentTimelinePin.svelte';

const dispatch = createEventDispatcher();

export let src: any;

// These are bound to properties of the video
let videoElem: any;
let time: number = 0;
let duration: number;
let paused: boolean = true;
let loop: boolean = false;
let videoCanvasContainer: any;
let vframeCalc: VideoFrame;

let debug_layout: boolean = false; // Set to true to show CSS layout boxes
let commentsWithTc: Proto3.Comment[] = [];  // Will be populated by the store once video is ready (=frame rate is known)


function refreshCommentPins(): void {
    // Make pins for all comments with timecode
    commentsWithTc = [];
    allComments.subscribe(comments => {
        for (let c of comments) { if (c.comment.timecode) { commentsWithTc.push(c.comment); } }
        commentsWithTc = commentsWithTc.sort((a, b) => {
            if (!a.timecode || !b.timecode) { return 0; }
            return a.timecode.localeCompare(b.timecode);  // Sort by SMPTE timecode = sort by string
        });
    });
}

function send_collab_report(): void {
    if ($collabId) {
        let drawing = paused ? getScreenshot() : undefined;
        let report: Proto3.client.ClientToServerCmd_CollabReport = {
            paused: videoElem.paused,
            loop: videoElem.loop,
            seekTimeSec: videoElem.currentTime,
            drawing,
            subtitleId: $curSubtitle?.id,
        };
        dispatch('collabReport', { report });
    }
}

let draw_color: string = "red";
let draw_board: any = null;
let draw_canvas: any = null;

function setPenColor(c: string): void {
    draw_color = c;
    draw_board.setLineColor(draw_color);
    draw_canvas.style.outline = "5px solid " + draw_color;
}

function prepare_drawing(): void
{
    if (!draw_board && videoElem.videoWidth>0)
    {
        $videoIsReady = true;

        vframeCalc = new VideoFrame({
            video: videoElem,
            frameRate: $videoFps,
            callback: function(response: any) { console.log(response); } });

        refreshCommentPins(); // Creates CommentTimelinePin components, now that we can calculate timecodes properly

        // Create the drawing board
        draw_canvas = document.createElement('canvas');
        draw_canvas.width = videoElem.videoWidth;
        draw_canvas.height = videoElem.videoHeight;
        draw_canvas.classList.add("absolute", "max-h-full", "max-w-full", "z-[100]");
        draw_canvas.style.cssText = 'outline: 5px solid red; outline-offset: -5px; cursor:crosshair; left: 50%; top: 50%; transform: translate(-50%, -50%);';

        // add mouse up listener to the canvas
        draw_canvas.addEventListener('mouseup', function(e: MouseEvent) {
            if (e.button == 0 && draw_canvas.style.visibility == "visible") {
                send_collab_report();
            }
        });

        videoCanvasContainer.appendChild(draw_canvas);

        draw_board = sdb_create(draw_canvas);
        draw_board.setLineSize(videoElem.videoWidth / 100);
        draw_board.setLineColor(draw_color);
        draw_canvas.style.visibility = "hidden"; // hide the canvas until the user clicks the draw button
    }
}


onMount(async () => {
    // Force the video to load
    if (!videoElem.videoWidth) { videoElem.load(); }
    prepare_drawing();
    offsetTextTracks();
    allComments.subscribe((_v) => { refreshCommentPins(); });
    curSubtitle.subscribe(() => { offsetTextTracks(); });
});

// Monitor video elem "loop" property in a timer.
// Couldn't find a way to bind to it directly.
setInterval(() => { loop = videoElem?.loop }, 500);


function handleMove(e: MouseEvent | TouchEvent, target: EventTarget|null) {
    if (!target) throw new Error("progress bar missing");
    if (!duration) return; // video not loaded yet
    if (e instanceof MouseEvent && !(e.buttons & 1)) return; // mouse not down
    videoElem.pause();
    const clientX = e instanceof TouchEvent ? e.touches[0].clientX : e.clientX;
    const { left, right } = (target as HTMLProgressElement).getBoundingClientRect();
    time = duration * (clientX - left) / (right - left);
    videoElem.currentTime = time;
    seekSideEffects();
    paused = true;
    send_collab_report();
}

let playback_request_source: string|undefined = undefined;

/// Start / stop playback
///
/// @param play  True to start, false to stop
/// @param request_source  ID of the source of the request, or undefined
/// @return  True if the playback state was changed
export function setPlayback(play: boolean, request_source: string|undefined): boolean {
    if (play == (!paused))
        return false;       // "no change"

    if (play) {
        seekSideEffects();
        videoElem.play();
    }
    else
        videoElem.pause();
    send_collab_report();

    playback_request_source = request_source;
    return true;
}

/// Get state of playback, and the source of the request that caused it
export function getPlaybackState(): {playing: boolean, request_source: string|undefined} {
    return {playing: !paused, request_source: playback_request_source};
}

export function isLooping(): boolean {
    return loop;
}

function togglePlay() {
    let should_play = paused;
    setPlayback(should_play, "VideoPlayer");
}

function format_tc(seconds: number) : string {
    if (isNaN(seconds)) return '...';
    if (vframeCalc) {
        const fr = Math.floor(seconds * vframeCalc.frameRate);
        return `${vframeCalc.toSMPTE(fr)}`;
    }
    else if(seconds==0)
        return '--:--:--:--';
    else {
        const minutes = Math.floor(seconds / 60);
        seconds = Math.floor(seconds % 60);
        // Return zero padded
        if (seconds < 10) return `${minutes}:0${seconds}`;
        else return `${minutes}:${seconds}`;
    }
}

function format_frames(seconds: number) : string {
    if (isNaN(seconds)) return '';
    if (vframeCalc) {
        const fr = Math.floor(seconds * vframeCalc.frameRate);
        return `${fr}`;
    }
    else
        return '----';
}


export function getCurTime() {
    return videoElem.currentTime;
}

export function getCurTimecode() {
    return format_tc(time);
}

export function getCurFrame() {
    let fps = vframeCalc.fps ?? NaN;
    if (isNaN(fps)) console.error("getCurFrame(): VideoFrame not initialized or invalid fps");
    return Math.floor(time * fps);
}


function step_video(frames: number) {
    if (vframeCalc) {
        if (frames < 0) {
            vframeCalc.seekBackward(-frames, null);
        } else {
            vframeCalc.seekForward(frames, null);
        }
        seekSideEffects();
        send_collab_report();
    }
}

const INTERACTIVE_ELEMS = ['input', 'textarea', 'select', 'option', 'button'];
const INTERACTIVE_ROLES = ['textbox', 'combobox', 'listbox', 'menu', 'menubar', 'grid', 'dialog', 'alertdialog'];
const WINDOW_KEY_ACTIONS: {[key: string]: (e: KeyboardEvent)=>any} = {
        ' ':  () => togglePlay(),
        'ArrowLeft': () => step_video(-1),
        'ArrowRight': () => step_video(1),
        'ArrowUp': () => step_video(1),
        'ArrowDown': () => step_video(-1),
        'z': (e) => { if (e.ctrlKey) onDrawUndo(); },
        'y': (e) => { if (e.ctrlKey) onDrawRedo(); },
    };

function onWindowKeyPress(e: KeyboardEvent): void {
    let target = e.target as HTMLElement;

    // Skip if the user is in a keyboard interactive element
    if (target.isContentEditable)
        return;

    if (INTERACTIVE_ELEMS.includes(target.tagName.toLowerCase()) ||
            INTERACTIVE_ROLES.includes(target.getAttribute('role') ?? '-'))
        return;

    if (e.key in WINDOW_KEY_ACTIONS) {
        WINDOW_KEY_ACTIONS[e.key](e);
        e.preventDefault();
    }
}

function seekSideEffects() {
    draw_board?.clear();
    onToggleDraw(false);
    dispatch('seeked', {});
}

export function seekToSMPTE(smpte: string) {
    try {
        seekSideEffects();
        vframeCalc.seekToSMPTE(smpte);
    } catch(err) {
        acts.add({mode: 'warning', message: `Seek failed to: ${smpte}`, lifetime: 3});
    }
}

export function seekToFrame(frame: number) {
    try {
        seekSideEffects();
        vframeCalc.seekToFrame(frame);
    } catch(err) {
        acts.add({mode: 'warning', message: `Seek failed to: ${frame}`, lifetime: 3});
    }
}

// Audio control
let audio_volume = 50;
$:{
    if (videoElem)
        videoElem.volume = audio_volume/100; // Immediately changes video element volume
}

// These are called from PARENT component on user interaction
export function onToggleDraw(mode_on: boolean) {
    try {
        draw_board.clear();
        if (mode_on) {
            draw_canvas.style.outline = "5px solid " + draw_color;
            draw_canvas.style.cursor = "crosshair";
            var ctx = draw_canvas.getContext('2d');
            ctx.drawImage(videoElem, 0, 0);
            draw_canvas.style.visibility = "visible";
        } else {
            draw_canvas.style.visibility = "hidden";
        }
    } catch(err) {
        acts.add({mode: 'error', message: `Video loading not done? Cannot enable drawing.`, lifetime: 3});
    }
}

export function onColorSelect(color: string) {
    setPenColor(color);
}

export function onDrawUndo() {
    draw_board?.undo();
}

export function onDrawRedo() {
    draw_board?.redo();
}

// Capture current video frame + drawing as a data URL (base64 encoded image)
export function getScreenshot() : string
{
        let comb = document.createElement('canvas');
        comb.width  = videoElem.videoWidth;
        comb.height = videoElem.videoHeight;
        var ctx = comb.getContext('2d');
        if (!ctx) throw new Error("Cannot get canvas context");
        // ctx.drawImage(videoElem, 0, 0);   // Removed, as bgr frame capture is now done when draw mode is entered
        ctx.drawImage(draw_canvas, 0, 0);
        return comb.toDataURL("image/webp", 0.8);
}

export function collabPlay(seek_time: number, looping: boolean) {
    videoElem.loop = looping;
    videoElem.pause();
    time = seek_time;
    seekSideEffects();
    videoElem.play();
}

export function collabPause(seek_time: number, looping: boolean, drawing: string|undefined) {
    videoElem.loop = looping;
    if (!paused)
        videoElem.pause();
    if (time != seek_time) {
        time = seek_time;
        seekSideEffects();
    }
    if (drawing && getScreenshot() != drawing)
        setDrawing(drawing);
}

export async function setDrawing(drawing: string) {
    try {
        await draw_board.fillImageByDataURL(drawing, { isOverlay: false })
        draw_canvas.style.visibility = "visible";
        draw_canvas.style.cursor = "";
        draw_canvas.style.outline = "none";
    }
    catch(err) {
        acts.add({mode: 'error', message: `Failed to show image.`, lifetime: 3});
    }
}

function tcToDurationFract(timecode: string|undefined) {
    /// Convert SMPTE timecode to a fraction of the video duration (0-1)
    if (timecode === undefined) { throw new Error("Timecode is undefined"); }
    if (!vframeCalc) { return 0; }
    let pos = vframeCalc.toMilliseconds(timecode)/1000.0;
    return pos / duration;
}

// Input element event handlers
function onTimecodeEdited(e: Event) {
    seekToSMPTE((e.target as HTMLInputElement).value);
    send_collab_report();
}

function onFrameEdited(e: Event) {
    seekToFrame(parseInt((e.target as HTMLInputElement).value));
    send_collab_report();
}


let uploadSubtitlesButton: HTMLButtonElement;
function changeSubtitleUploadIcon(upload_icon: boolean) {
    if (upload_icon) {
        uploadSubtitlesButton.classList.remove('fa-closed-captioning');
        uploadSubtitlesButton.classList.add('fa-upload');
    } else {
        uploadSubtitlesButton.classList.remove('fa-upload');
        uploadSubtitlesButton.classList.add('fa-closed-captioning');
    }
}

let prev_subtitle: Proto3.Subtitle|null = null;
function toggleSubtitle() {
    // Dispatch to parent instead of setting directly, to allow collab sessions to sync
    if ($allSubtitles.find(s => s.id == prev_subtitle?.id) == undefined) {
        prev_subtitle = null;
    }
    if ($curSubtitle) {
        prev_subtitle = $curSubtitle;
        dispatch('change-subtitle', {id: null});
    } else {
        if (prev_subtitle) {
            dispatch('change-subtitle', {id: prev_subtitle.id});
        } else {
            dispatch('change-subtitle', {id: $allSubtitles[0]?.id});
        }
    }
}


// Offset the start/end times of all cues in all text tracks by $curSubtitle.timeOffset seconds.
// Called when the video is loaded, and when the subtitle changes.
function offsetTextTracks() {
    interface ExtendedVTTCue extends VTTCue {
        originalStartTime?: number;
        originalEndTime?: number;
    }

    const adjustCues = (track: TextTrack) => {
        const offset = $curSubtitle?.timeOffset || 0.0;
        if (!track.cues) {
            console.debug("adjustCues(): track has no cues");
            return;
        }
        console.debug("Offsetting cues on text tracks by", offset, "sec");
        Array.from(track.cues).forEach((c) => {
            const cue = c as ExtendedVTTCue;
            if (!cue.originalStartTime) {
                cue.originalStartTime = cue.startTime;
                cue.originalEndTime = cue.endTime;
            }
            cue.startTime = cue.originalStartTime + offset;
            cue.endTime = (cue.originalEndTime ??  (cue.originalStartTime+1))  + offset;
        });
    }

    if (!videoElem?.textTracks) {
        console.debug("offsetTextTracks(): videoElem has no textTracks");
        return;
    }

    Array.from(videoElem?.textTracks).forEach((t) => {
        const track = t as TextTrack;
        if (!track.cues || track.cues.length == 0) {
            // If the track has no cues, wait a bit and try again (load events don't seem to work as expected)
            console.debug("offsetTextTracks(): Track has no cues, checking again in 500ms");
            setTimeout(() => { offsetTextTracks(); }, 500);
        } else {
            adjustCues(track);
        }
    });
}
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<div
    on:keydown={onWindowKeyPress}
    class="w-full h-full flex flex-col object-contain"
    role="main"
>
	<div  class="flex-1 grid place-items-center relative min-h-[12em]"
			 style="{debug_layout?'border: 2px solid orange;':''}">
		<div bind:this={videoCanvasContainer} class="absolute h-full {debug_layout?'border-4 border-x-zinc-50':''}">
			<video
				transition:scale
				src="{src}"
				crossOrigin="anonymous"
				preload="auto"
				class="h-full w-full"
				style="opacity: {$videoIsReady ? 1.0 : 0}; transition-opacity: 1.0s;"
				bind:this={videoElem}
				on:loadedmetadata={prepare_drawing}
				on:click={togglePlay}
				bind:currentTime={time}
				bind:duration
				bind:paused>
                <track kind="captions"
                    src="{$curSubtitle?.playbackUrl}"
                    srclang="en"
                    label="{$curSubtitle?.title}"
                    on:loadedmetadata={offsetTextTracks}
                    default
                />
			</video>

			<!--    TODO: maybe show actively controlling collaborator's avatar like this?
			<div class="absolute top-0 left-0 w-full h-full z-1">
				<div class="flex-none w-6 h-6 block"><Avatar username="Username Here"/></div>
			</div>
		-->

		</div>
	</div>

	<div class="flex-none {debug_layout?'border-2 border-red-600':''}">

		<div class="flex-1 space-y-0 leading-none">
			<progress value="{(time / duration) || 0}"
				class="w-full h-[2em] hover:cursor-pointer"
				on:mousedown|preventDefault={(e)=>handleMove(e, e.target)}
				on:mousemove={(e)=>handleMove(e, e.target)}
				on:touchmove|preventDefault={(e)=>handleMove(e, e.target)}
			/>
			{#each commentsWithTc as item}
				<CommentTimelinePin
					id={item.id}
					username={item.usernameIfnull || item.userId || '?'}
					comment={item.comment}
					x_loc={tcToDurationFract(item.timecode)}
					on:click={(_e) => { dispatch('commentPinClicked', {id: item.id});}}
					/>
			{/each}
		</div>

		<!-- playback controls -->
		<div class="flex p-1">

			<!-- Play/Pause -->
			<span class="flex-1 text-left ml-8 space-x-3 text-l whitespace-nowrap">
				<button class="fa-solid fa-chevron-left" on:click={() => step_video(-1)} disabled={time==0} title="Step backwards" />
				<button class="w-4 fa-solid {paused ? (loop ? 'fa-repeat' : 'fa-play') : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
				<button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>

				<!-- Timecode -->
				<span class="flex-0 mx-4 text-sm font-mono">
					<input class="bg-transparent hover:bg-gray-700 w-32" value="{format_tc(time)}" on:change={(e) => onTimecodeEdited(e)}/>
					FR <input class="bg-transparent hover:bg-gray-700 w-16" value="{format_frames(time)}" on:change={(e) => onFrameEdited(e)}/>
				</span>
			</span>

            <!-- Closed captioning -->
            <span class="flex-0 text-center whitespace-nowrap">
                {#if $allSubtitles.length > 0}
                    <button
                        class={ $curSubtitle ? 'fa-solid fa-closed-captioning text-amber-600' : 'fa-solid fa-closed-captioning text-gray-400' }
                        title="Toggle closed captioning"
                        on:click={() => toggleSubtitle()}
                    />
                {:else}
                    <button bind:this={uploadSubtitlesButton}
                        class="fa-solid fa-closed-captioning text-gray-400" title="Upload subtitles"
                        on:mouseover={() => { changeSubtitleUploadIcon(true); }}
                        on:focus={() => { changeSubtitleUploadIcon(true); }}
                        on:mouseout={() => { changeSubtitleUploadIcon(false); }}
                        on:blur={() => { changeSubtitleUploadIcon(false); }}
                        on:click={() => { dispatch('uploadSubtitles', {}); }}
                    />
                {/if}
            </span>

			<!-- Audio volume -->
			<span class="flex-0 text-center whitespace-nowrap">
				<button
					class="fas {audio_volume>0 ? 'fa-volume-high' : 'fa-volume-mute'} mx-2"
					on:click="{() => audio_volume = audio_volume>0 ? 0 : 50}"
					/>
                <input class="mx-2" id="vol-control" type="range" min="0" max="100" step="1" bind:value={audio_volume}/>
			</span>

			<!-- Video duration -->
			<span class="flex-0 text-lg mx-4">{format_tc(duration)}</span>
		</div>
	</div>

</div>

<svelte:window on:keydown={onWindowKeyPress} />

<style>

button:disabled {
    opacity: 0.3;
}
progress::-webkit-progress-bar {
    background-color: rgba(0,0,0,0.2);
}
progress::-webkit-progress-value {
    background-color: rgba(255,255,255,0.6);
}

</style>
