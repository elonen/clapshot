<script lang="ts">

import { onMount, createEventDispatcher } from 'svelte';
import { curUsername, curUserPic, videoTitle, mediaFileId, videoOrigUrl, collabId, userMenuItems } from "@/stores";
import Avatar from '@/lib/Avatar.svelte';
import logo from "@/assets/clapshot-logo.svg";
import {latestProgressReports} from '@/stores';
  import type { MediaProgressReport } from '@/types';


const dispatch = createEventDispatcher();

// Watch for (transcoding) progress reports from server, and update progress bar if one matches this item.
let videoProgressMsg: string | undefined = undefined;

onMount(async () => {
	latestProgressReports.subscribe((reports: MediaProgressReport[]) => {
		videoProgressMsg = reports.find((r: MediaProgressReport) => r.mediaFileId === $mediaFileId)?.msg;
	});
});

function onClickUser(): void {
    if (!$userMenuItems) return;
    let user_menu = document.getElementById("user-menu");
    if (user_menu?.classList.contains("hidden")) {
        user_menu.classList.remove("hidden");
    } else {
        user_menu?.classList.add("hidden");
    }
}

function logoutBasicAuth(urlFor401: RequestInfo, redirUrl: string) {
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

const randomSessionId = Math.random().toString(36).substring(2, 15);

</script>


<nav class="px-5 py-2.5 rounded dark:bg-gray-900">

	<div class="flex">

		<!-- logo with "home" link -->
		<span class="flex-0">
			<a href="/" class="flex items-baseline cursor-pointer">
				<img src={logo} class="mr-3 h-6 sm:h-9 filter brightness-75" alt="Clapshot" />
				<span class="self-center mt-1 text-4xl whitespace-nowrap text-gray-400" style="font-family: 'Yanone Kaffeesatz', sans-serif;">CLAPSHOT</span>
			</a>
		</span>

		<!-- video info -->
		<div class="flex-1 justify-between">
			{#if $mediaFileId}
			<span class="grid grid-flow-row auto-rows-max items-center font-mono text-gray-600 mx-4">
					<h2 class=" text-lg text-center">
						{$mediaFileId}
						<a href="?vid={$mediaFileId}" class="text-gray-700 hover:text-gray-500"><i class="fas fa-share-square text-sm"></i></a>
						{#if $videoOrigUrl}
							<a href="{$videoOrigUrl}" download title="Download original file" class="text-gray-700 hover:text-gray-500"><i class="fas fa-download text-sm"></i></a>
						{/if}
						{#if $collabId}
							<a href="?vid={$mediaFileId}" class="text-green-500 hover:text-orange-600" title="Collaborative session active. Click to exit."><i class="fas fa-users text-sm"></i></a>
						{:else}
							<a href="?vid={$mediaFileId}&collab={randomSessionId}" title="Start collaborative session" class="text-gray-700 hover:text-gray-500"><i class="fas fa-user-plus text-sm"></i></a>
						{/if}
					</h2>
				<span class="mx-4 text-xs text-center">{$videoTitle}</span>
				{#if videoProgressMsg}
					<span class="text-cyan-800 mx-4 text-xs text-center">{videoProgressMsg}</span>
				{/if}
			</span>
			{/if}
		</div>


		<!-- Username & avatar-->
		<div class="flex-0" style="visibility: {$curUsername ? 'visible': 'hidden'}">
			<span class="flex w-auto items-center">
				<h6 class="flex-1 mx-4 text-gray-500 font-semibold">{$curUsername}</h6>
				<button id="user-button" class="flex-0 ring-4 ring-slate-800 text-sm rounded-full" on:click|preventDefault={onClickUser}>
					{#if $curUserPic || $curUsername}
					<div class="w-10 block"><Avatar username={$curUsername} /></div>
					{/if}
				</button>
			</span>

			<!-- floating user menu, hidden by default -->
			{#if $userMenuItems != undefined && $userMenuItems.length > 0 }
				<div id="user-menu" class="absolute right-0 w-48 mt-2 origin-top-right z-[200] bg-white border border-gray-200 divide-y divide-gray-100 rounded-md shadow-lg ring-1 ring-black ring-opacity-5 focus:outline-none hidden">
					<div class="py-1">
						{#each $userMenuItems as item}
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
	</div>
</nav>

<style>
@import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
