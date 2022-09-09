<script lang="ts">
  import svelteLogo from './assets/svelte.svg'
  import CommentCard from './lib/CommentCard.svelte'
  import NavBar from './lib/NavBar.svelte'
  import CommentInput from './lib/CommentInput.svelte';
</script>

<main>
  <NavBar />

  <div class="flex">
    <div class="flex-1 bg-green-900">
      <div class="grid justify-items-center p-4" style="width: 100%;">
        <div class="block bg-cyan-300 h-[50vh] aspect-[16/9]">
          Video here
        </div>
        <div class="block w-3/4">
          <CommentInput/>
        </div>
      </div>
    </div>
    <div class="flex-none basis-72 bg-gray-900 py-2 px-2 space-y-2 ml-8">
      <CommentCard indent={0} />
      <CommentCard indent={0}/>
      <CommentCard indent={1} />
      <CommentCard indent={2} />
      <CommentCard indent={0} />
      <CommentCard indent={0}/>
      <CommentCard indent={1} />
      <CommentCard indent={0}/>
      <CommentCard indent={1} />
      <CommentCard indent={0}/>
    </div>
  </div>

</main>

<style>
</style>