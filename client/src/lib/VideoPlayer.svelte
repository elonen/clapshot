<script lang="ts">

  import {VideoFrame, FrameRates} from './VideoFrame.js';
  import {Notifications, acts} from '@tadashi/svelte-notification'
  import {create as sdb_create} from "simple-drawing-board";
  import {onMount} from 'svelte';
  
  import {video_is_ready, video_fps} from '../stores.js';

  export let src: any;

// These values are bound to properties of the video
  let video_elem: any;
	let time: number = 0;
	let duration: number;
  let paused: boolean = true;

  let video_canvas_container: any;

  let vframe_calc: VideoFrame;

 
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
      draw.canvas.style.border = "6px solid " + this._color;
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
      ctx.drawImage(video_elem, 0, 0);
      ctx.drawImage(this._canvas, 0, 0);
      return comb_canvas.toDataURL("image/webp", 0.8);
    }

    try_create_all() : void
    {
      if (!this._board && video_elem.videoWidth>0)
      {
        console.log("Creating drawing board");

        $video_is_ready = true;

        vframe_calc = new VideoFrame({
          video: video_elem,
          frameRate: $video_fps,
          callback: function(response) { console.log(response); } });

        // Create the drawing board
        this._canvas = document.createElement('canvas');
        this._canvas.width = video_elem.videoWidth;
        this._canvas.height = video_elem.videoHeight;
        this._canvas.style.cssText = 'border: 6px solid red; cursor:crosshair; opacity: 1.0; position: absolute; top: 0; left: 0; z-index: 1000; width: 100%; height: 100%;';
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
    draw.board?.clear();
	}

	function togglePlay() {
    if (paused) video_elem.play(); else video_elem.pause();
    draw.board?.clear();
	}

	function format(seconds: number) : string {
		if (isNaN(seconds)) return '...';
    if (vframe_calc)
      return `${vframe_calc.toSMPTE(seconds * vframe_calc.fps)}`;
    else if(seconds==0)
      return '--:--:--:--';
    else {
      const minutes = Math.floor(seconds / 60);
      seconds = Math.floor(seconds % 60);
      if (seconds < 10) seconds = '0' + seconds;
      return `${minutes}:${seconds}`;
    }
	}

  export function getCurTimecode() {
    return format(time);
  }

  function step_video(frames: number) {
    if (vframe_calc) {
      if (frames < 0) {
        vframe_calc.seekBackward(-frames, null);
      } else {
        vframe_calc.seekForward(frames, null);
      }
    }
    draw.board?.clear();
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
        video_elem.currentTime += 1;
        step_video(0);
        break;
      case 40: // down
        video_elem.currentTime -= 1;
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

  export function seekToTimecode(timecode: string) {
    try {
      vframe_calc.seekTo({ SMPTE: timecode });
      draw.board?.clear();
    } catch(err) {
      acts.add({mode: 'warning', message: `Seek failed to: ${timecode}`, lifetime: 3});
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
      if (mode_on) {
        draw.board.clear();
        draw.canvas.style.border = "6px solid " + draw.color;
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
      console.log("setDrawing");
      await draw.board.fillImageByDataURL(drawing, { isOverlay: false })
      draw.canvas.style.visibility = "visible";
      draw.canvas.style.border = "none";
    }
    catch(err) {
      acts.add({mode: 'error', message: `Failed to show image.`, lifetime: 3});
    }
  }


</script>

<div on:keydown={onWindowKeyPress}>

  <div bind:this={video_canvas_container} class="block relative">
    <video
      src="{src}"
      crossOrigin="anonymous"
      preload="auto"
      width="1920" height="1080"
      style="opacity: {$video_is_ready ? 1.0 : 0}; transition: 1.0s;"
      bind:this={video_elem}
      on:loadedmetadata={draw.try_create_all}
      on:click={togglePlay}
      bind:currentTime={time}
      bind:duration
      bind:paused>
      <track kind="captions">
    </video>
  </div>
  
	<div>
		<progress value="{(time / duration) || 0}"
      class="w-full h-[2em] hover:cursor-pointer"
      on:mousedown|preventDefault={handleMove}
      on:mousemove={handleMove}
      on:touchmove|preventDefault={handleMove}
    />

		<div class="flex p-1">
			
      <!-- Timecode -->
      <input class="flex-0 text-lg mx-4 bg-transparent hover:bg-gray-700 w-32 font-mono" value="{format(time)}" on:change={(e) => seekToTimecode(e.target.value)}/>
 
      <!-- Audio volume -->
      <span class="flex-0 text-center whitespace-nowrap">
        <button
          class="fas {audio_volume>0 ? 'fa-volume-high' : 'fa-volume-mute'} mx-2"
          on:click="{() => audio_volume = audio_volume>0 ? 0 : 50}"
          />
          <input class="mx-2" id="vol-control" type="range" min="0" max="100" step="1" bind:value={audio_volume}/>
      </span>

      <!-- Play/Pause -->
			<span class="flex-1 text-left ml-16 space-x-3 text-xl whitespace-nowrap">
        <button class="fa-solid fa-chevron-left" on:click={() => step_video(-1)} disabled={time==0} title="Step backwards" />
        <button class="fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
        <button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>
      </span>


      <!-- Video duration -->
			<span class="flex-0 text-lg mx-4 ml-8">{format(duration)}</span>
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
