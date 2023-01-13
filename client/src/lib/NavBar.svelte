<script lang="ts">
import { createEventDispatcher } from 'svelte';
import Avatar from './Avatar.svelte';
import { cur_username, cur_user_pic, video_orig_filename, video_hash, video_progress_msg, collab_id } from "../stores.js";
import logo from "../assets/clapshot-logo.svg";

  const dispatch = createEventDispatcher();
  function onClickBanner() {
    dispatch("clear-all", {});
  }

  const random_session_id = Math.random().toString(36).substring(2, 15);

</script>

<nav class="px-5 py-2.5 rounded dark:bg-gray-900">
  
  <div class="flex">

    <!-- logo with "home" link -->
    <span class="flex-0">
      <a href="/" class="flex items-baseline cursor-pointer" on:click|preventDefault="{onClickBanner}">
        <img src={logo} class="mr-3 h-6 sm:h-9 filter brightness-75" alt="Clapshot" />
        <span class="self-center mt-1 text-4xl whitespace-nowrap text-gray-400" style="font-family: 'Yanone Kaffeesatz', sans-serif;">CLAPSHOT</span>
      </a>
    </span>

    <!-- video info -->
    <div class="flex-1 justify-between">
      {#if $video_hash}
      <span class="grid grid-flow-row auto-rows-max items-center font-mono text-gray-600 mx-4">
          <h2 class=" text-lg text-center">
            {$video_hash}
            <a href="?vid={$video_hash}" class="text-gray-700 hover:text-gray-500"><i class="fas fa-share-square text-sm"></i></a>
            {#if $collab_id}
              <a href="?vid={$video_hash}" class="text-green-500 hover:text-orange-600" title="Collaborative session active. Click to exit."><i class="fas fa-users text-sm"></i></a>
            {:else}
              <a href="?vid={$video_hash}&collab={random_session_id}" title="Start collaborative session" class="text-gray-700 hover:text-gray-500"><i class="fas fa-user-plus text-sm"></i></a>
            {/if}
          </h2>
        <span class="mx-4 text-xs text-center">{$video_orig_filename}</span>  
        {#if $video_progress_msg}
          <span class="text-cyan-800 mx-4 text-xs text-center">{$video_progress_msg}</span>
        {/if}
      </span>
      {/if}
    </div>


    <!-- Username & avatar-->
    <div class="flex-0" style="visibility: {$cur_username ? 'visible': 'hidden'}">
      <span class="flex w-auto items-center">
        <h6 class="flex-1 mx-4 text-gray-500 font-semibold">{$cur_username}</h6>
        <button class="flex-0 ring-4 ring-slate-800 text-sm rounded-full" on:click|preventDefault={onClickBanner}>
          {#if $cur_user_pic || $cur_username}
          <div class="w-10 block"><Avatar userFullName={$cur_username} src={$cur_user_pic} /></div>
          {/if}
        </button>
      </span>
    </div>

  </div>
</nav>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
