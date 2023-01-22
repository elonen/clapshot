<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { scale, slide } from "svelte/transition";
  import Avatar from './Avatar.svelte';
  import { all_comments, cur_username, cur_user_id, cur_user_pic } from '../stores.js';

  const dispatch = createEventDispatcher();

  export let id: string = "";
  export let parent_id: string = null;
  export let username: string = "";
  export let user_id: string = "";
  export let comment: string = "";
  export let edited: string = null;
  export let avatar_url: string = "";
  export let indent: number = 0;
  export let timecode: string = "";
  export let drawing_data: string = "";

  let editing = false;
  let comment_edit_field: any = null;

  let show_actions: boolean = false;
 
  let show_reply: boolean = false;
  let reply_input: HTMLInputElement;
  
  function onTimecodeClick(tc) {
    dispatch("display-comment", {'timecode': tc, 'drawing': drawing_data});
  }

function onClickDeleteComment() {
  var result = confirm("Delete comment?");
  if (result) {
    dispatch("delete-comment", {'comment_id': id});
  }
}

function onReplySubmit() {
  if (reply_input.value != "") 
  {
    dispatch("reply-to-comment", {'parent_id': id, 'comment_text': reply_input.value});
    reply_input.value = "";
    show_reply = false;
  }
}

function callFocus(elem) {
  elem.focus();
}

function onEditFieldKeyUp(e) {
  if (e.key == "Enter") {
    console.log("Enter pressed");
    editing = false;
    if (comment != "") 
      dispatch("edit-comment", {'comment_id': id, 'comment_text': comment});
  }  
}

</script>

<div transition:scale
  id="comment_card_{id}"
  class="block overflow-clip rounded-lg bg-gray-800 {!!timecode ? 'hover:bg-gray-700' : ''} shadow-lg shadow-black"
  style="margin-left: {indent*1.5}em"
  on:mouseenter="{() => show_actions=true}"
  on:mouseleave="{() => show_actions=false}"
  on:click = "{() => {if (timecode) onTimecodeClick(timecode);}}">


  <div class="flex mx-2 pt-3">
    <div class="flex-none w-9 h-9 block"><Avatar userFullName="{username}" src="{avatar_url}" /></div>
    <h5 class="flex-1 ml-3 text-gray-500 self-end">{username}</h5>
    <span class="flex-none hidden text-xs font-mono">[{id}@{parent_id}]</span>
    <span
      on:click="{() => { onTimecodeClick(timecode) }}"
      class="pl-2 flex-0 text-xs italic whitespace-nowrap text-yellow-700 hover:text-yellow-500 hover:underline cursor-pointer self-end">
        {#if drawing_data && drawing_data != ""}
          <i class="fas fa-pen"></i>
        {/if}
        {timecode ? timecode : ""}
    </span>
  </div>

  <div class="p-2" lang="en">
    {#if editing}
      <input class="w-full outline-dashed bg-slate-500" type="text" use:callFocus bind:value="{comment}" on:keyup={onEditFieldKeyUp} on:blur="{(e)=>{editing=false;}}" />
    {:else}
      <p class="text-gray-300 text-base hyphenate">
        {comment}
        {#if edited}
          <span class="text-xs italic text-gray-500"> (edited)</span>
        {/if}
      </p>
    {/if}
  </div>

  {#if show_actions}  
  <div class="p-2 flex place-content-end" transition:slide="{{ duration: 200 }}">
    <button class="border rounded-lg px-1 placeholder: ml-2 text-sm border-cyan-500 text-cyan-500" on:click={()=>show_reply=true}>Reply</button>
    {#if user_id == $cur_user_id || $cur_user_id == "admin"}
      <button class="border rounded-lg px-1 ml-2 text-sm border-cyan-600 text-cyan-600" on:click="{(e)=>{editing=true;}}">Edit</button>      
      <button class="border rounded-lg px-1 ml-2 text-sm border-red-300 text-red-300" on:click={onClickDeleteComment}>Del</button>
    {/if}
  </div>
  {/if}

  {#if show_reply}
    <form class="p-2" on:submit|preventDefault={onReplySubmit}>
      <input 
        class="w-full border p-1 rounded bg-gray-900"
        type="text" placeholder="Your reply..."
        use:callFocus
        bind:this={reply_input}
        on:blur="{()=>show_reply=false}" />
    </form>
  {/if}

</div>


<style>
  .hyphenate {
    -webkit-hyphens: auto;
    -moz-hyphens: auto;
    -ms-hyphens: auto;
    hyphens: auto;
    word-break: break-word;
  }

  .highlight { 
    animation-name: glow;
    animation-duration: .4s;
    border: blue 1px solid;
  }

  @keyframes glow {
    0% { box-shadow: 0px 0px 15px 10px rgba(255, 255, 0, 1); }
    100% { box-shadow: 0px 0px 0px 0px rgba(255, 255, 0, 1); }
  }
</style>