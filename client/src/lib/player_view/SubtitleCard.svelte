<script lang="ts">

import { createEventDispatcher } from 'svelte';
import { scale, slide } from "svelte/transition";
import { curSubtitle, curUserId, curUserIsAdmin, videoOwnerId } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';

const dispatch = createEventDispatcher();

export let sub: Proto3.Subtitle;
export let isDefault: boolean = false;

let showEditor: boolean = false;

function doSave() {
    dispatch("update-subtitle", {sub, isDefault});
    showEditor = false;
}

function doDelete() {
    dispatch("delete-subtitle", {id: sub.id});
    showEditor = false;
}

</script>


<div transition:scale class="flex flex-nowrap space-x-1 text-sm whitespace-nowrap justify-between items-center text-gray-400 w-full">
    <button
        class="flex-grow text-left hover:text-white {sub.id == $curSubtitle?.id ? 'text-amber-600' : 'text-gray-400'} overflow-hidden"
        on:click={() => dispatch("change-subtitle", {id: sub.id})}
        title={sub.origFilename}
        style="text-overflow: ellipsis; white-space: nowrap;"
    >
        <i class="fa {sub.id == $curSubtitle?.id ? 'fa-eye' : 'fa-eye-slash' }"></i>
        <span class="text-ellipsis"><strong>{sub.languageCode.toUpperCase()}</strong> – {sub.title}</span>
    </button>
    {#if $videoOwnerId == $curUserId || $curUserIsAdmin}
    <span class="flex-shrink-0">
        <button class="fa fa-pencil hover:text-white" title="Edit subtitle" on:click={() => { showEditor = !showEditor; }}></button>
    </span>
    {/if}
</div>

{#if showEditor}
<form class="space-y-2" transition:slide="{{ duration: 200 }}">
    <div>
        <label for="title" class="block text-sm font-medium text-gray-600">Title</label>
        <input id="title" type="text" bind:value={sub.title} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <div>
        <label for="language_code" class="block text-sm font-medium text-gray-600">Language Code</label>
        <input id="language_code" minlength="2" maxlength="3"  type="text" bind:value={sub.languageCode} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <div>
        <label for="time_offset" class="block text-sm font-medium text-gray-600">Time Offset</label>
        <input id="time_offset" type="number" bind:value={sub.timeOffset} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <div>
        <label for="isDefault" class="block text-sm font-medium text-gray-600">Default Subtitle</label>
        <input id="isDefault" type="checkbox" bind:checked={isDefault} class="mt-1 block rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <button type="button" class="border rounded-lg px-1 ml-2 text-sm border-cyan-600 text-cyan-600" on:click={doSave}>Save</button>
    <button type="button" class="border rounded-lg px-1 ml-2 text-sm border-red-300 text-red-300" on:click={doDelete}>del</button>
    <button type="button" class="border rounded-lg px-1 ml-2 text-sm border-gray-600 text-gray-600" on:click={() => { showEditor=false; }}>Cancel</button>
</form>
{/if}
