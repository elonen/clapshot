<script lang="ts">

  import {VideoFrame} from './VideoFrame';
  import {acts} from '@tadashi/svelte-notification'
  import {create as sdb_create} from "simple-drawing-board";
  import {onMount} from 'svelte';
  import {scale} from "svelte/transition";

  import {all_comments, video_is_ready, video_fps, collab_id} from '../stores';

  import {createEventDispatcher} from 'svelte';
  import CommentTimelinePin from './CommentTimelinePin.svelte';

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
  
  function refreshCommentPins(): void {
    // Make pins for all comments with timecode
    commentsWithTc = [];
    all_comments.subscribe(comments => {
      for (let c of comments) { if (c.timecode) { commentsWithTc.push(c); } }
      commentsWithTc = commentsWithTc.sort((a, b) => a.timecode - b.timecode);
    });
  }

  function send_collab_report(): void {
    if ($collab_id) {
      let drawing = paused ? getScreenshot() : null;
      dispatch('collabReport', {paused: video_elem.paused, seek_time: video_elem.currentTime, drawing: drawing});
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
    if (!draw_board && video_elem.videoWidth>0)
    {
      $video_is_ready = true;

      vframe_calc = new VideoFrame({
        video: video_elem,
        frameRate: $video_fps,
        callback: function(response: any) { console.log(response); } });

      refreshCommentPins(); // Creates CommentTimelinePin components, now that we can calculate timecodes properly

      // Create the drawing board
      draw_canvas = document.createElement('canvas');
      draw_canvas.width = video_elem.videoWidth;
      draw_canvas.height = video_elem.videoHeight;
      draw_canvas.classList.add("absolute", "max-h-full", "max-w-full", "z-[100]");
      draw_canvas.style.cssText = 'outline: 5px solid red; outline-offset: -5px; cursor:crosshair; left: 50%; top: 50%; transform: translate(-50%, -50%);';

      // add mouse up listener to the canvas
      draw_canvas.addEventListener('mouseup', function(e: MouseEvent) {
        if (e.button == 0 && draw_canvas.style.visibility == "visible") {
          send_collab_report();
        }
      });

      video_canvas_container.appendChild(draw_canvas);

      draw_board = sdb_create(draw_canvas);
      draw_board.setLineSize(video_elem.videoWidth / 100);
      draw_board.setLineColor(draw_color);
      draw_canvas.style.visibility = "hidden"; // hide the canvas until the user clicks the draw button
    }
  }


	onMount(async () => {
    // Force the video to load
    if (!video_elem.videoWidth) { video_elem.load(); }
    prepare_drawing();
    all_comments.subscribe((_v) => { refreshCommentPins(); });
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
      // Return zero padded
      if (seconds < 10) return `${minutes}:0${seconds}`;
      else return `${minutes}:${seconds}`;
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

  const INTERACTIVE_ELEMS = ['input', 'textarea', 'select', 'option', 'button'];
  const INTERACTIVE_ROLES = ['textbox', 'combobox', 'listbox', 'menu', 'menubar', 'grid', 'dialog', 'alertdialog'];
  const WINDOW_KEY_ACTIONS = {
      ' ': togglePlay,
      'ArrowLeft': () => step_video(-1),
      'ArrowRight': () => step_video(1),
      'ArrowUp': () => step_video(1),
      'ArrowDown': () => step_video(-1),
      'z': (e: KeyboardEvent) => { if (e.ctrlKey) onDrawUndo(); },
      'y': (e: KeyboardEvent) => { if (e.ctrlKey) onDrawRedo(); },
    };

  function onWindowKeyPress(e: KeyboardEvent): void {
    let target = e.target as HTMLElement;

    // Skip if the user is in a keyboard interactive element
    if (target.isContentEditable)
      return;

    if (INTERACTIVE_ELEMS.includes(target.tagName.toLowerCase()) ||
        INTERACTIVE_ROLES.includes(target.getAttribute('role')))
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
      draw_board.clear();
      if (mode_on) {
        draw_canvas.style.outline = "5px solid " + draw_color;
        draw_canvas.style.cursor = "crosshair";
        var ctx = draw_canvas.getContext('2d');
        ctx.drawImage(video_elem, 0, 0);
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
      comb.width  = video_elem.videoWidth;
      comb.height = video_elem.videoHeight;
      var ctx = comb.getContext('2d');
      // ctx.drawImage(video_elem, 0, 0);   // Removed, as bgr frame capture is now done when draw mode is entered
      ctx.drawImage(draw_canvas, 0, 0);
      return comb.toDataURL("image/webp", 0.8);
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

  function tcToDurationFract(timecode: string) {
    /// Convert SMPTE timecode to a fraction of the video duration (0-1)
    if (!vframe_calc) { return 0; }
    let pos = vframe_calc.toMilliseconds(timecode)/1000.0;
    return pos / duration;
  }

  // Input element event handlers

  function onPosEdited(e: Event, fmt: string) {
    seekTo((e.target as HTMLInputElement).value, fmt);
    send_collab_report();
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
        on:loadedmetadata={prepare_drawing}
        on:click={togglePlay}
        bind:currentTime={time}
        bind:duration
        bind:paused>
        <track kind="captions">
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
        on:mousedown|preventDefault={handleMove}
        on:mousemove={handleMove}
        on:touchmove|preventDefault={handleMove}
      />
      {#each commentsWithTc as item}
        <CommentTimelinePin
          id={item.id}
          username={item.username}
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
        <button class="w-4 fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} title="Play/Pause" />
        <button class="fa-solid fa-chevron-right" on:click={() => step_video(1)} title="Step forwards"/>

        <!-- Timecode -->
        <span class="flex-0 mx-4 text-sm font-mono">
          <input class="bg-transparent hover:bg-gray-700 w-32" value="{format_tc(time)}" on:change={(e) => onPosEdited(e, 'SMPTE')}/>
          FR <input class="bg-transparent hover:bg-gray-700 w-16" value="{format_frames(time)}" on:change={(e) => onPosEdited(e, 'frame')}/>
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
			<span class="flex-0 text-lg mx-4">{format_tc(duration)}</span>
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
