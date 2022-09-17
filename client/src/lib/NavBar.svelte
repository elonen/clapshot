<script lang="ts">
import { createEventDispatcher } from 'svelte';
import Avatar from './Avatar.svelte';
import { cur_username, cur_user_pic, video_orig_filename, video_hash } from "../stores.js";
import logo from "../assets/clapshot-logo.svg";

  const dispatch = createEventDispatcher();
  function onClickBanner() {
    dispatch("clear-all", {});
  }

  let url = document.URL;

</script>

<nav class="bg-white border-gray-200 px-2 sm:px-4 py-2.5 rounded dark:bg-gray-900">
  
  <div class="container flex flex-wrap justify-between items-center mx-auto">

    <a href="/" class="flex items-center cursor-pointer" on:click|preventDefault="{onClickBanner}">
      <img src={logo} class="mr-3 h-6 sm:h-9 filter brightness-75" alt="Clapshot" />
      <span class="self-center text-4xl whitespace-nowrap text-gray-300 align-text-bottom" style="font-family: 'Yanone Kaffeesatz', sans-serif;">CLAPSHOT</span>
    </a>
    
    <div class="flex items-center md:order-2" style="visibility: {$cur_username ? 'visible': 'hidden'}">
      <h6 class="mx-4 text-gray-500 font-semibold">{$cur_username}</h6>
      <button class="flex mr-3 text-sm bg-gray-800 rounded-full md:mr-0 focus:ring-4 focus:ring-gray-300 dark:focus:ring-gray-600" on:click|preventDefault={onClickBanner}>
            {#if $cur_user_pic || $cur_username}
              <Avatar userFullName={$cur_username} src={$cur_user_pic} />
            {/if}
      </button>

    </div>

    <div class="justify-between items-center w-full md:flex md:w-auto md:order-1">
      {#if $video_hash}
      <span class="grid grid-flow-row auto-rows-max font-mono text-gray-600 mx-4">
          <h2 class=" text-lg text-center">
            {$video_hash}
            <a href="?vid={$video_hash}" class="fas fa-share-square text-sm text-gray-700 hover:text-gray-500" />
          </h2>
        <span class="mx-4 text-sm text-center">{$video_orig_filename}</span>  
      </span>
      {/if}      
    </div>

  </div>
</nav>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
