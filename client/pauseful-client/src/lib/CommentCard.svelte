<script lang="ts">
  let username: string = "Test User";
  let comment: string = "This is a test comment";
  export let indent: number = 0;
</script>

<div class="block rounded-lg shadow-lg bg-gray-800" style="margin-left: {indent*1.5}em">

  <div class="block flex mx-2 pt-1">
    <img src="https://mdbootstrap.com/img/new/avatars/2.jpg" class="flex-0 rounded-full w-8 h-8" alt="" loading="lazy" />
    <h5 class="px-2 flex-1 text-gray-500 self-end">{username}</h5>
    <span class="pl-2 flex-0 text-xs italic text-yellow-800 self-end">5min ago</span>
  </div>

  <div class="p-2">
    <p class="text-gray-300 text-base">{comment}</p>
  </div>

</div>
