<script lang="ts">

import { onMount, createEventDispatcher } from 'svelte';
import { curUsername, curUserPic, curVideo, mediaFileId, collabId, userMenuItems } from "@/stores";
import Avatar from '@/lib/Avatar.svelte';
import {latestProgressReports, clientConfig} from '@/stores';
import type { MediaProgressReport } from '@/types';
import { Dropdown, DropdownItem, DropdownDivider, DropdownHeader } from 'flowbite-svelte';
import EDLImport from './tools/EDLImport.svelte';
import { ChevronRightOutline } from 'flowbite-svelte-icons';
import { Modal } from 'flowbite-svelte';

const dispatch = createEventDispatcher();
let loggedOut = false;

// Watch for (transcoding) progress reports from server, and update progress bar if one matches this item.
let videoProgressMsg: string | undefined = undefined;

onMount(async () => {
	latestProgressReports.subscribe((reports: MediaProgressReport[]) => {
		videoProgressMsg = reports.find((r: MediaProgressReport) => r.mediaFileId === $mediaFileId)?.msg;
	});
});



function logoutBasicAuth() {
    // This is a bit tricky, as HTTP basic auth wasn't really designed for logout.
    // Logout URL is expected to return 401 status code and return a Clear-Site-Data header.
    // Additionally, send bad credentials in case 401 and Clear-Site-Data wasn't enough to forget the credentials.
    // After that, disconnect websocket and show a modal to prompt user to reload the page.

    const logoutUrl = $clientConfig?.logout_url || "/logout";
	const nonce = Math.random().toString(36).substring(2, 15);

    console.log("Making request to " + logoutUrl + " with bad creds...");
    fetch(logoutUrl, {method:'GET', headers: {'Authorization': 'Basic ' + btoa('logout_user__'+nonce+':bad_pass__'+nonce)}})
        .then(res => {
            console.log("Logout response: " + res.status + " - " + res.statusText);
            if (res.status === 401) {
                console.log("Logout successful.");
				dispatch('basic-auth-logout', {});
				loggedOut = true;	// Show modal
            } else {
                alert("Basic auth logout failed.\nStatus code from " + logoutUrl + ": " + res.status + " (not 401)");
            }
        })
        .catch(error => {
            console.log("Error logging out: " + error);
        })
}

function showAbout() {
	alert("Clapshot Client version " + process.env.CLAPSHOT_CLIENT_VERSION + "\n" +
		"\n" +
		"Visit the project page at:\n" +
		"https://github.com/elonen/clapshot\n");
}

async function copyToClipboard() {
	const urlParams = `?vid=${$mediaFileId}`;
	const currentUrl = window.location.href.split('?')[0]; // remove existing query parameters
	const fullUrl = currentUrl + urlParams;
	try {
		await navigator.clipboard.writeText(fullUrl);
		alert('Link copied to clipboard.\nSend it to reviewers who have user accounts here.');
	} catch (err) {
		console.error('Failed to copy link: ', err);
	}
}

const randomSessionId = Math.random().toString(36).substring(2, 15);


let isEDLImportOpen = false;
function addEDLComments(event: any) {
	console.debug("addEDLComments", event.detail);
	dispatch('add-comments', event.detail);
}


</script>

<nav class="px-5 py-2.5 rounded dark:bg-gray-900">

	<div class="flex">

		<!-- logo with "home" link -->
		<span class="flex-0">
			<a href="/" class="flex items-baseline cursor-pointer">
				<img src="{$clientConfig ? ($clientConfig?.logo_url || "clapshot-logo.svg") : ""}" class="mr-3 h-6 sm:h-9 filter brightness-75" alt="{$clientConfig ? ($clientConfig.app_title || "Clapshot") : ""}" />
				<span class="self-center mt-1 text-4xl whitespace-nowrap text-gray-400" style="font-family: 'Yanone Kaffeesatz', sans-serif;">{($clientConfig ? ($clientConfig.app_title || "Clapshot") : "").toUpperCase()}</span>
			</a>
		</span>

		<!-- video info -->
		<div class="flex-1 justify-between">
			{#if $mediaFileId}
			<span class="grid grid-flow-row auto-rows-max items-center text-gray-600 mx-4">
					<h2 class=" text-lg text-center">
						<span class="font-mono">{$mediaFileId}</span>

						<div class="relative inline-block text-left">
							<button type="button"
							  	class="inline-flex justify-center w-full rounded-md shadow-sm px-2 py-0.5 {$collabId ? 'bg-green-500' : 'bg-gray-800'} text-sm font-medium text-gray-500 hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
								aria-haspopup="true" aria-expanded="true"
							>
								<i class="fas fa-bars"></i>
							</button>

						<Dropdown class="w-64 text-sm">
							<DropdownItem on:click={copyToClipboard}><i class="fas fa-share-square"></i> Share to logged in users</DropdownItem>
							{#if $curVideo?.origUrl}
								<DropdownItem href="{$curVideo?.origUrl}" download title="Download original file"><i class="fas fa-download"></i> Download original</DropdownItem>
							{/if}
							{#if $collabId}
								<DropdownItem href="?vid={$mediaFileId}" class="text-green-400"><i class="fas fa-users"></i> Leave collaborative Session</DropdownItem>
							{:else}
								<DropdownItem href="?vid={$mediaFileId}&collab={randomSessionId}" title="Start collaborative session"><i class="fas fa-user-plus"></i> Start Collaborative Session</DropdownItem>
							{/if}

							<DropdownItem>
								<i class="fas fa-cog"></i> Experimental tools
								<ChevronRightOutline class="w-6 h-6 ms-2 float-right" />
							</DropdownItem>
							<Dropdown placement="right-start" class="w-64 text-sm">
								<DropdownItem on:click={() => isEDLImportOpen = true}><i class="fas fa-file-import"></i> Import EDL as Comments</DropdownItem>
								<EDLImport bind:isOpen={isEDLImportOpen} on:add-comments={addEDLComments}/>
							</Dropdown>
						</Dropdown>

					</h2>
				<span class="mx-4 text-xs text-center">{$curVideo?.title}</span>
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
				<button id="user-button" class="flex-0 ring-4 ring-slate-800 text-sm rounded-full" aria-haspopup="true" aria-expanded="true">
					{#if $curUserPic || $curUsername}
					<div class="w-10 block"><Avatar username={$curUsername} /></div>
					{/if}
				</button>
			</span>

			{#if $userMenuItems != undefined && $userMenuItems.length > 0 }
				<Dropdown class="w-44 text-sm">
					{#each $userMenuItems as item}
						<DropdownItem>
						{#if item.type === "logout-basic-auth" }
							<DropdownItem on:click={() => logoutBasicAuth()}>{item.label}</DropdownItem>
						{:else if item.type === "about"}
							<DropdownItem on:click={showAbout}>{item.label}</DropdownItem>
						{:else if item.type === "divider"}
							<DropdownDivider />
						{:else if item.type === "url"}
							<DropdownItem href={item.data}>{item.label}</DropdownItem>
						{:else}
							<DropdownItem>UNKNOWN item.type '{item.type}'</DropdownItem>
						{/if}
						</DropdownItem>
					{/each}
				</Dropdown>
			{/if}
		</div>
	</div>
</nav>

<Modal title="Logged out" dismissable={false} bind:open={loggedOut} class="w-96">
	<p><i class="fas fa fa-sign-in"></i> Reload page to log in again.</p>
</Modal>


<style>
@import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
