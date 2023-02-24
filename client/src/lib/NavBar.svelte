<script lang="ts">
import { createEventDispatcher } from 'svelte';
import Avatar from './Avatar.svelte';
import { cur_username, cur_user_pic, video_title, video_hash, video_progress_msg, collab_id, user_menu_items } from "../stores.js";
import { all_popup_hide_funcs } from '../stores.js';

import logo from "../assets/clapshot-logo.svg";

  const dispatch = createEventDispatcher();
  function onClickBanner() {
    dispatch("clear-all", {});
  }

  function onClickUser() {
    if (!user_menu_items) return;
    const user_menu = document.getElementById("user-menu");
    if (user_menu.classList.contains("hidden")) {
      $all_popup_hide_funcs.forEach((func) => { func(); }); // hide all popups first
      user_menu.classList.remove("hidden");
    } else {
      user_menu.classList.add("hidden");
    }
  }

  function hideMenu() {
    const user_menu = document.getElementById("user-menu");
    user_menu.classList.add("hidden");
  }
  $all_popup_hide_funcs.push(hideMenu);

  
  function logoutBasicAuth(urlFor401, redirUrl) {
    // Try to log out of basic auth by making a request to /logout and expect 401.
    // This is a bit tricky, as HTTP basic auth wasn't really designed for logout.
    console.log("Making logout request to " + urlFor401 + " and redirecting to " + redirUrl + "...");
    dispatch('basic-auth-logout', {});
    fetch(urlFor401)
      .then(res => {
        console.log("Logout response: " + res.status + " - " + res.statusText);
        if (res.status === 401) {
          console.log("Logout successful.");
          dispatch('basic-auth-logout', {});
          setTimeout(function () { window.location.href = redirUrl; }, 1000);
        } else {
          alert("Basic auth logout failed.\nStatus code from " + urlFor401 + ": " + res.status + " (not 401)");
        }
      })
      .catch(error => {
        console.log("Error logging out: " + error);
      })
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
        <span class="mx-4 text-xs text-center">{$video_title}</span>
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
        <button id="user-button" class="flex-0 ring-4 ring-slate-800 text-sm rounded-full" on:click|preventDefault={onClickUser}>
          {#if $cur_user_pic || $cur_username}
          <div class="w-10 block"><Avatar userFullName={$cur_username} src={$cur_user_pic} /></div>
          {/if}
        </button>
      </span>

      <!-- floating user menu, hidden by default -->
      {#if $user_menu_items != undefined && $user_menu_items.length > 0 }
        <div id="user-menu" class="absolute right-0 w-48 mt-2 origin-top-right z-[200] bg-white border border-gray-200 divide-y divide-gray-100 rounded-md shadow-lg ring-1 ring-black ring-opacity-5 focus:outline-none hidden">
          <div class="py-1">
            {#each $user_menu_items as item}
              {#if item.type === "logout-basic-auth" }
                <button on:click|preventDefault={() => logoutBasicAuth('/logout', '/')} class="block text-left px-4 py-2 w-full text-sm text-gray-700 hover:bg-gray-100" role="menuitem">{item.label}</button>
              {:else if item.type === "divider"}
                <div class="border-t border-gray-100 my-1"></div>
              {:else if item.type === "url"}
                <a href="{item.data}" class="block text-left px-4 py-2 w-full text-sm text-gray-700 hover:bg-gray-100" role="menuitem">{item.label}</a>
              {:else}
                <em>UNKNOWN item.type '{item.type}'</em>
              {/if}
            {/each}
          </div>
        </div>
      {/if}

  </div>
</nav>


<style>
  @import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
