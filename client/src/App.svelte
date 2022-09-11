<script lang="ts">

  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import VideoPlayer from './lib/VideoPlayer.svelte';
  import CommentInput from './lib/CommentInput.svelte';
  import { all_comments, cur_username, cur_user_pic } from './stores.js';
  import example_video from './assets/big_buck_bunny_720p_5mb.mp4';


  $all_comments = [
		{ id: crypto.randomUUID(), comment: "Huono. Tehkää parempi.", username: "Aulis Apulainen", avatar_url: "https://mdbootstrap.com/img/new/avatars/1.jpg", indent: 0 },
		{ id: crypto.randomUUID(), comment: "Huono. Tehkää parempi.", username: "Aulis Apulainen", avatar_url: "https://mdbootstrap.com/img/new/avatars/1.jpg", indent: 1 },
		{ id: crypto.randomUUID(), comment: "Huono. Tehkää parempi.", username: "Aulis Apulainen", avatar_url: "https://mdbootstrap.com/img/new/avatars/1.jpg", indent: 2 },
		{ id: crypto.randomUUID(), comment: "Huono. Tehkää parempi.", username: "Aulis Apulainen", avatar_url: "https://mdbootstrap.com/img/new/avatars/1.jpg", indent: 0 },
];

  console.log($all_comments);

  let video_player: VideoPlayer;
  let comment_container: HTMLDivElement;
  let comment_input: CommentInput;

  function onCommentInputButton(e) {
    console.log("Comment Input Button Clicked: " + e.detail.action);
    if (e.detail.action == "send")
    {
      if (e.detail.comment_text != "")
      {
        $all_comments[$all_comments.length] = {
          id: crypto.randomUUID(),
          indent: 0,
          username: $cur_username,
          avatar_url: $cur_user_pic,
          comment: e.detail.comment_text,
          timecode: e.detail.is_timed ? video_player.getCurTimecode() : "",
          drawing_data: video_player.getDrawing()
        };        
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

  function onDisplayComment(e) {
    video_player.seekToTimecode(e.detail.timecode);
    comment_input.forceDrawMode(false);  // Close draw mode while showing (drawing from a saved) comment
    if (e.detail.drawing)    
      video_player.setDrawing(e.detail.drawing);
  }

  function onSeekToTimecode(e) {
    console.log("Seek to timecode: " + e.detail.timecode);
    video_player.seekToTimecode(e.detail.timecode);
  }


</script>

<main>

  <div class="flex">
    <div class="flex-1 bg-black">
      <div class="grid justify-items-center" style="width: 100%;">
        <div class="w-full mb-4"><NavBar /></div>

        <div class="block bg-cyan-900 ">
          <VideoPlayer bind:this={video_player} src={example_video}/>
        </div>
        <div class="block w-full p-4">
          <CommentInput bind:this={comment_input} on:button-clicked={onCommentInputButton} />
        </div>
      </div>
    </div>
    <div class="flex-none basis-128 bg-gray-900 py-2 px-2 space-y-2 ml-2 h-screen overflow-y-scroll" bind:this={comment_container}>

      {#each $all_comments as item}
        <CommentCard {...item} on:display-comment={onDisplayComment}/>
	    {/each}
    </div>
  </div>


</main>

<style>
</style>