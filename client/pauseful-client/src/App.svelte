<script lang="ts">

  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import VideoPlayer from './lib/VideoPlayer.svelte';
  import CommentInput from './lib/CommentInput.svelte';
</script>

<main>

  <div class="flex">
    <div class="flex-1 bg-black">
      <div class="grid justify-items-center" style="width: 100%;">
        <div class="w-full mb-4"><NavBar /></div>

        <div class="block bg-cyan-900 ">
          <VideoPlayer src="src/assets/big_buck_bunny_720p_5mb.mp4"/>
        </div>
        <div class="block w-full p-4">
          <CommentInput/>
        </div>
      </div>
    </div>
    <div class="flex-none basis-72 bg-gray-900 py-2 px-2 space-y-2 ml-2 h-screen overflow-y-scroll">
      {#each [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16] as i}
        <CommentCard indent={i%3} />
      {/each}
    </div>
  </div>

</main>

<style>
</style>