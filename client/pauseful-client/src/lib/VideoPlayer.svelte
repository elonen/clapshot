<script lang="ts">

  import {VideoFrame, FrameRates} from './VideoFrame.js';
  import {Notifications, acts} from '@tadashi/svelte-notification'
 
  export let src: any;

// These values are bound to properties of the video
  let video_elem: any;
	let time: number = 0;
	let duration: number;
  let paused: boolean = true;

  

  var vframe: VideoFrame;

  function onVideoLoaded() {
    vframe = new VideoFrame({
      video: video_elem,
      frameRate: FrameRates.film, /// TODO: THIS NEEDS TO BE READ FROM SERVER - no way to get this from the video element
      callback: function(response) { console.log(response); }
    });
  }

  
	function handleMove(e) {
		if (!duration) return; // video not loaded yet
		if (e.type !== 'touchmove' && !(e.buttons & 1)) return; // mouse not down

		const clientX = e.type === 'touchmove' ? e.touches[0].clientX : e.clientX;
		const { left, right } = this.getBoundingClientRect();
		time = duration * (clientX - left) / (right - left);
	}

	function togglePlay() {
    if (paused) video_elem.play(); else video_elem.pause();
	}

	function format(seconds) {
		if (isNaN(seconds)) return '...';

    if (vframe) { return `${vframe.toSMPTE(seconds * vframe.fps)}`; }
    

		const minutes = Math.floor(seconds / 60);
		seconds = Math.floor(seconds % 60);
		if (seconds < 10) seconds = '0' + seconds;

		return `${minutes}:${seconds}`;
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
        vframe.seekBackward(1, null);
        break;
      case 39: // right
        vframe.seekForward(1, null);
        break;
      case 38: // up
        video_elem.currentTime += 1;
        break;
      case 40: // down
      video_elem.currentTime -= 1;
        break;
    }
    e.preventDefault();
  }

  function onTimeCodeEdited(e: any) {
    try {
      vframe.seekTo({ SMPTE: e.target.value });
    } catch(err) {
      acts.add({mode: 'warning', message: `Seek failed to: ${e.target.value}`, lifetime: 3});
    }
  }

</script>

<div on:keydown={onWindowKeyPress}>
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

	<div class="controls">
		<progress value="{(time / duration) || 0}"
      class="w-full h-[2em]"
      on:mousemove={handleMove}
      on:touchmove|preventDefault={handleMove}
    />

		<div class="flex p-1">
			
      <input class="text-lg mx-4 flex-0 bg-transparent w-32" value="{format(time)}" on:change={onTimeCodeEdited}/>

			<span class="flex-1 text-center space-x-3 text-xl">
          <button class="fa-solid fa-chevron-left" on:click={() => vframe.seekBackward(1, ()=>{})} />
          <button class="fa-solid {paused ? 'fa-play' : 'fa-pause'}" on:click={togglePlay} />
          <button class="fa-solid fa-chevron-right" on:click={() => vframe.seekForward(1, ()=>{})} />
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
