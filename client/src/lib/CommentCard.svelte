<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { scale, slide } from "svelte/transition";
  import { all_comments, cur_username, cur_user_pic } from '../stores.js';

  const dispatch = createEventDispatcher();

  export let id: string = "";
  export let username: string = "Test User";
  export let comment: string = "This is a test comment";
  export let avatar_url: string = "";
  export let indent: number = 0;
  export let timecode: string = "";
  export let drawing_data: string = null;

  let show_actions: boolean = false;
 
  let show_reply: boolean = false;
  let reply_input: HTMLInputElement;
  
  function onTimecodeClick(tc) {
    dispatch("display-comment", {'timecode': tc, 'drawing': drawing_data});
  }

function deleteComment() {
  var result = confirm("Delete comment?");
  if (result) {
		$all_comments = $all_comments.filter(function(value, index, arr) { 
			if (value.id != id) return value;
		});    
  }
}

function onReplySubmit() {
  if (reply_input.value != "") 
  {
    let this_idx = $all_comments.findIndex(function(value, index, arr) {
      if (value.id === id) return index;
    });
    // Reply at the end of thread
    for (let i=this_idx+1; i<$all_comments.length; i++)
      if ($all_comments[i].indent > indent )
        this_idx = i;

    console.log(this_idx);
    $all_comments.splice(this_idx+1, 0, {
      id: crypto.randomUUID(),
      indent: indent + 1,
      username: $cur_username,
      avatar_url: $cur_user_pic,
      comment: reply_input.value,
      timecode: ""
    });
    $all_comments = $all_comments;
    reply_input.value = "";
    show_reply = false;
  }
}

</script>

<div 
  transition:scale
  class="block rounded-lg shadow-lg bg-gray-800 {!!timecode ? 'hover:bg-gray-700' : ''}"
  style="margin-left: {indent*1.5}em"
  on:mouseenter="{() => show_actions=true}"
  on:mouseleave="{() => show_actions=false}"
  on:click = "{() => {if (timecode) onTimecodeClick(timecode);}}"
  >

  <div class="block flex mx-2 pt-1">
    <img src={avatar_url} class="flex-0 rounded-full w-8 h-8" alt="" loading="lazy" />
    <h5 class="px-2 flex-1 text-gray-500 self-end">{username}</h5>
    <span 
      on:click="{() => { onTimecodeClick(timecode) }}"
      class="pl-2 flex-0 text-xs italic text-yellow-700 hover:text-yellow-500 hover:underline cursor-pointer self-end">
      {timecode}
    </span>
  </div>

  <div class="p-2">
    <p class="text-gray-300 text-base">{comment}</p>
  </div>

  {#if show_actions}
  <div class="p-2 flex place-content-end" transition:slide="{{ duration: 200 }}">
    <button class="border rounded-lg px-1 text-gray-200 text-base ml-2 text-sm border-green-300 text-green-300"  on:click={() => show_reply=true}>Reply</button>
    <button class="opacity-25  border rounded-lg px-1 text-gray-200 text-base ml-2 text-sm border-orange-300 text-orange-300">Edit</button>
    <button class="border rounded-lg px-1 text-gray-200 text-base ml-2 text-sm border-red-300 text-red-300" on:click={deleteComment}>Del</button>
  </div>
  {/if}

  {#if show_reply}
    <form class="p-2" on:submit|preventDefault={onReplySubmit}>
      <input 
        class="w-full border p-1 rounded bg-gray-900"
        type="text" placeholder="Your reply..."
        autofocus
        bind:this={reply_input}
        on:blur="{()=>show_reply=false}" />
    </form>
  {/if}

</div>
