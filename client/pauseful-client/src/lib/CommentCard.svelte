<script lang="ts">
  let username: string = "Test User";
  let comment: string = "This is a test comment";
  export let indent: number = 0;
</script>

<div class="flex justify-center ml-{indent*8}">
  <div class="block rounded-lg shadow-lg bg-gray-800 max-w-sm text-left container mx-auto">
    <div class="block flex flex-nowrap border-b border-gray-800">
      <img src="https://mdbootstrap.com/img/new/avatars/2.jpg" class="rounded-full p-2 w-10 h-10" alt="" loading="lazy" />
      {#if username}
        <span class="p-2">
            <h2 class="font-bold text-gray-400">{username}</h2>
        </span>
      {/if}
      <span class="p-2 text-xs italic text-yellow-800">5min ago</span>

    </div>
    <div class="p-2">
      <p class="text-gray-500 text-base">
        {comment}
      </p>
    </div>
  </div>
</div>
