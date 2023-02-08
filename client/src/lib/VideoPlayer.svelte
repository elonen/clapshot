<script lang="ts">

  import {VideoFrame, FrameRates} from './VideoFrame.js';
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import {create as sdb_create} from "simple-drawing-board";
  import {onMount} from 'svelte';
  import {fade, slide, scale} from "svelte/transition";

  import {all_comments, video_is_ready, video_fps, collab_id} from '../stores.js';
  import Avatar from './Avatar.svelte';

  import {createEventDispatcher} from 'svelte';
  import CommentTimelinePin from './CommentTimelinePin.svelte';
  import { each } from 'svelte/internal';
  const dispatch = createEventDispatcher();

  export let src: any;

// These values are bound to properties of the video
  let video_elem: any;
	let time: number = 0;
	let duration: number;
  let paused: boolean = true;
  let video_canvas_container: any;
  let vframe_calc: VideoFrame;

  let debug_layout: boolean = false; // Set to true to show CSS layout boxes

  let commentsWithTc = [];  // Will be populated by the store once video is ready (=frame rate is known)

  function refreshCommentPins() {
    // Make pins for all comments with timecode
    commentsWithTc = [];
    all_comments.subscribe(comments => {
      for (let c of comments) { if (c.timecode) { commentsWithTc.push(c); } }
      commentsWithTc = commentsWithTc.sort((a, b) => a.timecode - b.timecode);
    });
  }


  function send_collab_report() {
    if ($collab_id) {
      let drawing = paused ? getDrawing(true) : null;
      dispatch('collabReport', {paused: video_elem.paused, seek_time: video_elem.currentTime, drawing: drawing});
    }
  }

  class Draw
  {
    constructor() {
      this._color = "red";
      this._board = null;
      this._canvas = null;
    }

    get color() { return this._color; }
    set color(color: string) {
      this._color = color;
      draw.board.setLineColor(this._color);
      draw.canvas.style.outline = "5px solid " + this._color;
    }

    get board() {
      this.try_create_all();
      return this._board;
    }

    get canvas() {
      this.try_create_all();
      return this._canvas;
    }
    
    isEmpty(): bool 
    {
      if (!this._board || !this._canvas) return true;
      const blankCanvas = document.createElement('canvas');
      blankCanvas.width = this._canvas.width;
      blankCanvas.height = this._canvas.height;
      return this._canvas.toDataURL() === blankCanvas.toDataURL()
    }

    // Returns the drawing as a data URL, or an empty string if the drawing is empty.
    // If including_empty is true, then the data URL is returned even if the
    // drawing is empty -- this is useful for sending screenshot of current
    // video frame even without the drawing, mainly used as a work-around
    // for sharing exact frame with others since HTML video element seeking
    // is currently (Jan 2023) not necessarily frame-precise. 
    getDataUrl(including_empty: boolean = False): string
    {
      if (this.isEmpty() && !including_empty)
        return "";
      let comb_canvas = document.createElement('canvas');
      comb_canvas.width  = video_elem.videoWidth;
      comb_canvas.height = video_elem.videoHeight;
      var ctx = comb_canvas.getContext('2d');
      // ctx.drawImage(video_elem, 0, 0);   // Removed, as frame capture is now done when draw mode is entered
      ctx.drawImage(this._canvas, 0, 0);
      return comb_canvas.toDataURL("image/webp", 0.8);
    }

    try_create_all() : void
    {
      if (!this._board && video_elem.videoWidth>0)
      {
        //console.log("Creating drawing board");

        $video_is_ready = true;

        vframe_calc = new VideoFrame({
          video: video_elem,
          frameRate: $video_fps,
          callback: function(response) { console.log(response); } });

        refreshCommentPins(); // Creates CommentTimelinePin components, now that we can calculate timecodes properly

        // Create the drawing board
        this._canvas = document.createElement('canvas');
        this._canvas.width = video_elem.videoWidth;
        this._canvas.height = video_elem.videoHeight;
        this._canvas.classList.add("absolute", "max-h-full", "max-w-full", "z-[1000]");
        this._canvas.style.cssText = 'outline: 5px solid red; outline-offset: -5px; cursor:crosshair; left: 50%; top: 50%; transform: translate(-50%, -50%);';

        // add mouse up listener to the canvas
        this._canvas.addEventListener('mouseup', function(e) {
          if (e.button == 0 && draw.canvas.style.visibility == "visible") {
            console.log("Mouse up");
            send_collab_report();
          }
        });

        video_canvas_container.appendChild(this._canvas);

        this._board = sdb_create(this._canvas);
        this._board.setLineSize(video_elem.videoWidth / 100);
        this._board.setLineColor(this.color);
        this._canvas.style.visibility = "hidden"; // hide the canvas until the user clicks the draw button
      }
    }
  }
  let draw = new Draw();

	onMount(async () => {
    // Force the video to load
    if (!video_elem.videoWidth) { video_elem.load(); }
    draw.try_create_all();
    all_comments.subscribe((v) => { refreshCommentPins(); });
	});


	function handleMove(e: any) {
		if (!duration) return; // video not loaded yet
		if (e.type !== 'touchmove' && !(e.buttons & 1)) return; // mouse not down
    video_elem.pause();
		const clientX = e.type === 'touchmove' ? e.touches[0].clientX : e.clientX;
		const { left, right } = this.getBoundingClientRect();
		time = duration * (clientX - left) / (right - left);
    video_elem.currentTime = time;
    seekSideEffects();
    paused = true;
    send_collab_report();
	}

	function togglePlay() {
    if (paused) {
      seekSideEffects();
      video_elem.play();
    }
    else video_elem.pause();
    send_collab_report();
	}

	function format_tc(seconds: number) : string {
		if (isNaN(seconds)) return '...';
    if (vframe_calc) {
      const fr = Math.floor(seconds * vframe_calc.frameRate);
      return `${vframe_calc.toSMPTE(fr)}`;
    }
    else if(seconds==0)
      return '--:--:--:--';
    else {
      const minutes = Math.floor(seconds / 60);
      seconds = Math.floor(seconds % 60);
      if (seconds < 10) seconds = '0' + seconds;
      return `${minutes}:${seconds}`;
    }
	}

	function format_frames(seconds: number) : string {
		if (isNaN(seconds)) return '';
    if (vframe_calc) {
      const fr = Math.floor(seconds * vframe_calc.frameRate);
      return `${fr}`;
    }
    else
      return '----';
	}


  export function getCurTime() {
    return video_elem.currentTime;
  }

  export function getCurTimecode() {
    return format_tc(time);
  }

  export function getCurFrame() {
    return Math.floor(time * vframe_calc.fps);
  }


  function step_video(frames: number) {
    if (vframe_calc) {
      if (frames < 0) {
        vframe_calc.seekBackward(-frames, null);
      } else {
        vframe_calc.seekForward(frames, null);
      }
      seekSideEffects();
      send_collab_report();
    }
  }

  function onWindowKeyPress(e: any) {
    var event = document.all ? window.event : e;

    // Skip if the user is in a keyboard interactive element
    if (e.target.isContentEditable)
      return;
    switch (e.target.tagName.toLowerCase()) {
      case "input":
      case "textarea":
      case "select":
      case "button":
        return;
    }
    //console.log(e);
    switch(event.keyCode) {
      case 32: // space
        togglePlay();
        break;
      case 37: // left
        step_video(-1);
        break;
      case 39: // right
      step_video(1);
        break;
      case 38: // up
        time += 1;
        step_video(0);
        break;
      case 40: // down
        time -= 1;
        step_video(0);
        break;
      case 90: // z
        if (e.ctrlKey) {
          onDrawUndo();
          break;
        }
      case 89: // y
        if (e.ctrlKey) {
          onDrawRedo();
          break;
        }
    }
    e.preventDefault();
  }

  function seekSideEffects() {
    draw.board?.clear();
    onToggleDraw(false);
    dispatch('seeked', {});
  }

  export function seekTo(value: string, fmt: string) {
    // fmt should be either "SMPTE" or "frame"
    try {
      seekSideEffects();
      let ops = new Object();
      ops[fmt] = value;
      vframe_calc.seekTo(ops);
    } catch(err) {
      acts.add({mode: 'warning', message: `Seek failed to: ${fmt} ${value}`, lifetime: 3});
    }
  }

  // Audio control
  let audio_volume = 50;
	$:{
    if (video_elem)
      video_elem.volume = audio_volume/100; // Immediately changes video element volume
	}

  // These are called from PARENT component on user interaction
  export function onToggleDraw(mode_on: boolean) {
    try {
      draw.board.clear();
      if (mode_on) {
        draw.canvas.style.outline = "5px solid " + draw.color;
        draw.canvas.style.cursor = "crosshair";
        var ctx = draw.canvas.getContext('2d');
        ctx.drawImage(video_elem, 0, 0);
        draw.canvas.style.visibility = "visible";
      } else {
        draw.canvas.style.visibility = "hidden";
      }
    } catch(err) {
      acts.add({mode: 'error', message: `Video loading not done? Cannot enable drawing.`, lifetime: 3});
    }
  }

  export function onColorSelect(color: string) {
    draw.color = color;
  }
  
  export function onDrawUndo() {
    draw.board?.undo();
  }

  export function onDrawRedo() {
    draw.board?.redo();
  }

  export function getDrawing(including_empty: boolean = false) : string | null {
    return draw.getDataUrl(including_empty);
  }

  export function collabPlay(seek_time: number) {
    video_elem.pause();
    time = seek_time;
    seekSideEffects();
    video_elem.play();
  }

  export function collabPause(seek_time: number, drawing: string) {
    if (!paused)
      video_elem.pause();
    if (time != seek_time) {
      time = seek_time;
      seekSideEffects();
    }
    if (drawing && drawing != getDrawing(true))
      setDrawing(drawing);
  }

  export async function setDrawing(drawing: string) {
    try {
      await draw.board.fillImageByDataURL(drawing, { isOverlay: false })
      draw.canvas.style.visibility = "visible";
      draw.canvas.style.cursor = "";
      draw.canvas.style.outline = "none";
    }
    catch(err) {
      acts.add({mode: 'error', message: `Failed to show image.`, lifetime: 3});
    }
  }

  function tcToDurationFract(timecode: string) {
    /// Convert SMPTE timecode to a fraction of the video duration (0-1)
    if (!vframe_calc) { return 0; }
    let pos = vframe_calc.toMilliseconds(timecode)/1000.0;
    return pos / duration;
  }

</script>

<div on:keydown={onWindowKeyPress} class="w-full h-full flex flex-col object-contain">

  <div  class="flex-1 grid place-items-center relative min-h-[12em]"
       style="{debug_layout?'border: 2px solid orange;':''}">
    <div bind:this={video_canvas_container} class="absolute h-full {debug_layout?'border-4 border-x-zinc-50':''}">
      <video
        transition:scale
        src="{src}"
        crossOrigin="anonymous"
        preload="auto"
        class="h-full w-full"
        style="opacity: {$video_is_ready ? 1.0 : 0}; transition-opacity: 1.0s;"
        bind:this={video_elem}
        on:loadedmetadata={draw.try_create_all}
        on:click={togglePlay}
        bind:currentTime={time}
        bind:duration
        bind:paused>
        <track kind="captions">
      </video>

      <!--    TODO: maybe show actively controlling collaborator's avatar like this?
      <div class="absolute top-0 left-0 w-full h-full z-1">
        <div class="flex-none w-6 h-6 block"><Avatar userFullName="Username Here"/></div>
      </div>
    -->

    </div>
  </div>
  
	<div class="flex-none {debug_layout?'border-2 border-red-600':''}">

    <div class="flex-1 space-y-0 leading-none">
      <progress value="{(time / duration) || 0}"
        class="w-full h-[2em] hover:cursor-pointer"
        on:mousedown|preventDefault={handleMove}
        on:mousemove={handleMove}
        on:touchmove|preventDefault={handleMove}
      />
      {#each commentsWithTc as item}
        <CommentTimelinePin
          id={item.id},
          username={item.username},
          comment={item.comment},
          avatar_url={item.avatar_url},
          x_loc={tcToDurationFract(item.timecode).toString()}
          on:click={(e) => { dispatch('commentPinClicked', {id: item.id});}}
          />
      {/each}
    </div>

    <!-- playback controls -->
		<div class="flex p-1">
			
      <!-- Play/Pause -->
			<span class="flex-1 text-left ml-8 space-x-3 text-l whitespace-nowrap">
        <button class="fa-solid fa-chevron-left" on:click={() => step_video(-1)} disabled={time==0} title="Step backwards" />
        <button class="w-4 fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
        <button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>

        <!-- Timecode -->
        <span class="flex-0 mx-4 text-sm font-mono">
          <input class="bg-transparent hover:bg-gray-700 w-32" value="{format_tc(time)}" on:change={(e) => {seekTo(e.target.value, 'SMPTE'); send_collab_report();}}/>
          FR <input class="bg-transparent hover:bg-gray-700 w-16" value="{format_frames(time)}" on:change={(e) => {seekTo(e.target.value, 'frame'); send_collab_report();}}/>
        </span>

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
			<span class="flex-0 text-lg mx-4 mx-8">{format_tc(duration)}</span>
		</div>
	</div>
  
</div>

<svelte:window on:keydown={onWindowKeyPress} />

<style>
  @import '@fortawesome/fontawesome-free/css/all.min.css';
  
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
