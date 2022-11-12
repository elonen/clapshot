<script lang="ts">

  import {VideoFrame, FrameRates} from './VideoFrame.js';
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import {create as sdb_create} from "simple-drawing-board";
  import {onMount} from 'svelte';
  import {fade, slide, scale} from "svelte/transition";

  import {video_is_ready, video_fps} from '../stores.js';

  import {createEventDispatcher} from 'svelte';
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

    getDataUrl(): string
    {
      if (this.isEmpty())
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

        // Create the drawing board
        this._canvas = document.createElement('canvas');
        this._canvas.width = video_elem.videoWidth;
        this._canvas.height = video_elem.videoHeight;
        this._canvas.classList.add("absolute", "max-h-full", "max-w-full", "z-[1000]");
        this._canvas.style.cssText = 'outline: 5px solid red; outline-offset: -5px; cursor:crosshair; left: 50%; top: 50%; transform: translate(-50%, -50%);';

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
	});


	function handleMove(e: any) {
		if (!duration) return; // video not loaded yet
		if (e.type !== 'touchmove' && !(e.buttons & 1)) return; // mouse not down
		const clientX = e.type === 'touchmove' ? e.touches[0].clientX : e.clientX;
		const { left, right } = this.getBoundingClientRect();
		time = duration * (clientX - left) / (right - left);
    seekSideEffects();
	}

	function togglePlay() {
    if (paused) {
      seekSideEffects();
      video_elem.play();
    }
    else video_elem.pause();
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

  export function getDrawing() {
    return draw.getDataUrl();
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
    </div>
  </div>
  
	<div class="flex-none {debug_layout?'border-2 border-red-600':''}">
		<progress value="{(time / duration) || 0}"
      class="w-full h-[2em] hover:cursor-pointer"
      on:mousedown|preventDefault={handleMove}
      on:mousemove={handleMove}
      on:touchmove|preventDefault={handleMove}
    />

    <!-- playback controls -->
		<div class="flex p-1">
			
      <!-- Play/Pause -->
			<span class="flex-1 text-left ml-8 space-x-3 text-l whitespace-nowrap">
        <button class="fa-solid fa-chevron-left" on:click={() => step_video(-1)} disabled={time==0} title="Step backwards" />
        <button class="w-4 fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
        <button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>

        <!-- Timecode -->
        <span class="flex-0 mx-4 text-sm font-mono">
          <input class="bg-transparent hover:bg-gray-700 w-32" value="{format_tc(time)}" on:change={(e) => seekTo(e.target.value, 'SMPTE')}/>
          FR <input class="bg-transparent hover:bg-gray-700 w-16" value="{format_frames(time)}" on:change={(e) => seekTo(e.target.value, 'frame')}/>
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
