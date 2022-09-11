<script lang="ts">

  import {VideoFrame, FrameRates} from './VideoFrame.js';
  import {Notifications, acts} from '@tadashi/svelte-notification'
 
  import { create as sdb_create } from "simple-drawing-board";
  import { onMount } from 'svelte';


  export let src: any;

// These values are bound to properties of the video
  let video_elem: any;
	let time: number = 0;
	let duration: number;
  let paused: boolean = true;

  let video_canvas_container: any;

  let vframe_calc: VideoFrame;
  let draw_board: any;
  let draw_canvas: any;
  let draw_color: string = "red";

  function onVideoLoaded()
  {
    vframe_calc = new VideoFrame({
      video: video_elem,
      frameRate: FrameRates.film, /// TODO: THIS NEEDS TO BE READ FROM SERVER - no way to get this from the video element
      callback: function(response) { console.log(response); } });


    // Create the drawing board
    draw_canvas = document.createElement('canvas');
    draw_canvas.width = video_elem.videoWidth;
    draw_canvas.height = video_elem.videoHeight;
    draw_canvas.style.cssText = 'border: 6px solid red; cursor:crosshair; opacity: 0.66; position: absolute; top: 0; left: 0; z-index: 1000; width: 100%; height: 100%;';
    video_canvas_container.appendChild(draw_canvas);

    draw_board = sdb_create(draw_canvas);
    draw_board.setLineSize(video_elem.videoWidth / 100);
    draw_board.setLineColor(draw_color);
    draw_canvas.style.visibility = "hidden"; // hide the canvas until the user clicks the draw button
  }


	function handleMove(e) {
		if (!duration) return; // video not loaded yet
		if (e.type !== 'touchmove' && !(e.buttons & 1)) return; // mouse not down

		const clientX = e.type === 'touchmove' ? e.touches[0].clientX : e.clientX;
		const { left, right } = this.getBoundingClientRect();
		time = duration * (clientX - left) / (right - left);
    draw_board.clear();
	}

	function togglePlay() {
    if (paused) video_elem.play(); else video_elem.pause();
    draw_board.clear();
	}

	function format(seconds) {
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
    draw_board.clear();
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

  function onTimeCodeEdited(e: any) {
    seekToTimecode(e.target.value)
  }

  export function seekToTimecode(timecode: string) {
    try {
      vframe_calc.seekTo({ SMPTE: timecode });
      draw_board.clear();
    } catch(err) {
      acts.add({mode: 'warning', message: `Seek failed to: ${timecode}`, lifetime: 3});
    }
  }




  // These are called from PARENT component on user interaction
  export function onToggleDraw(mode_on: boolean) {
    if (mode_on) {
      draw_board.clear();
      draw_canvas.style.border = "6px solid " + draw_color;
      draw_canvas.style.visibility = "visible";
    } else {
      draw_canvas.style.visibility = "hidden";
    }
  }

  export function onColorSelect(color: string)
  {
    draw_color = color;
    draw_board.setLineColor(draw_color);
    draw_canvas.style.border = "6px solid " + draw_color;
  }
  
  export function onDrawUndo() {
    draw_board.undo();
  }

  export function onDrawRedo() {
    draw_board.redo();
  }

  export function getDrawing() {
    return draw_board.toDataURL();
  }

  export async function setDrawing(drawing: string) {
    console.log("setDrawing");
    await draw_board.fillImageByDataURL(drawing, { isOverlay: false })
    draw_canvas.style.visibility = "visible";
    draw_canvas.style.border = "none";
  }


</script>

<div on:keydown={onWindowKeyPress}>

  <div bind:this={video_canvas_container} class="block relative">
    <video
      src="{src}"
      on:loadedmetadata={onVideoLoaded}
      on:click={togglePlay}
      bind:this={video_elem}
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
			
      <input class="text-lg mx-4 flex-0 bg-transparent hover:bg-gray-700 w-32 font-mono" value="{format(time)}" on:change={onTimeCodeEdited}/>

			<span class="flex-1 text-center space-x-3 text-xl">
          <button class="fa-solid fa-chevron-left" on:click={() => step_video(-1)} disabled={time==0} title="Step backwards" />
          <button class="fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
          <button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>
      </span>
			<span class="text-lg mx-4  flex-0">{format(duration)}</span>
		</div>
	</div>

  <Notifications />

  
</div>

<svelte:window on:keydown={onWindowKeyPress} />

<style>
  @import url('https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.2.0/css/all.min.css');

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
