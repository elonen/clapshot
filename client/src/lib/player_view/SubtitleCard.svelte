<script lang="ts">

import { createEventDispatcher } from 'svelte';
import { scale, slide } from "svelte/transition";
import { curSubtitle, curUserId, curUserIsAdmin, curVideo, subtitleEditingId } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';

const dispatch = createEventDispatcher();

export let sub: Proto3.Subtitle;
export let isDefault: boolean = false;


function doSave() {
    dispatch("update-subtitle", {sub, isDefault});
    $subtitleEditingId = null;
}

function doDelete() {
    dispatch("delete-subtitle", {id: sub.id});
    $subtitleEditingId = null;
}

function toggleEditing() {
    $subtitleEditingId = ($subtitleEditingId == sub.id ? null : sub.id);
}

function handleKeyDown(event: { key: string; }) {
    if (event.key === 'Escape') {
        $subtitleEditingId = null;
    }
}

</script>


<div transition:scale class="flex flex-nowrap space-x-1 text-sm whitespace-nowrap justify-between items-center text-gray-400 w-full">
    <button
        class="flex-grow text-left hover:text-white {sub.id == $curSubtitle?.id ? 'text-amber-600' : 'text-gray-400'} overflow-hidden"
        on:click={() => dispatch("change-subtitle", {id: sub.id})}
        on:dblclick={toggleEditing}
        title={sub.origFilename}
        style="text-overflow: ellipsis; white-space: nowrap;"
    >
        <i class="fa {sub.id == $curSubtitle?.id ? 'fa-eye' : 'fa-eye-slash' }"></i>
        <span class="text-ellipsis"><strong>{sub.languageCode.toUpperCase()}</strong> – {sub.title}</span>
    </button>
    {#if $curVideo?.userId == $curUserId || $curUserIsAdmin}
    <span class="flex-shrink-0">
        <button class="fa {($subtitleEditingId==sub.id) ? "fa-angle-down" : "fa-angle-right"} hover:text-white" title="Edit subtitle" on:click={() => { toggleEditing(); }}></button>
    </span>
    {/if}
</div>

{#if $subtitleEditingId == sub.id}
<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<form class="space-y-2 p-2 mb-4 rounded-lg bg-gray-800 shadow-lg shadow-black" transition:slide="{{ duration: 200 }}" on:keydown={handleKeyDown}>
    <div>
        <label for="title" class="block text-sm font-medium text-gray-500">Title</label>
        <input id="title" type="text" bind:value={sub.title} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <div class="flex space-x-2">
        <div>
            <label for="language_code" class="block text-sm font-medium text-gray-500">
                Language code
                <a href="https://en.wikipedia.org/wiki/List_of_ISO_639_language_codes" target="_blank" class="text-xs text-gray-500 hover:text-gray-300"><i class="fas fa-circle-info"/></a>
            </label>
            <input id="language_code" minlength="2" maxlength="3"  type="text" bind:value={sub.languageCode} class="mt-1 block w-full uppercase font-mono rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        </div>
        <div>
            <label for="time_offset" class="block text-sm font-medium text-gray-500">Time offset (sec)</label>
            <input id="time_offset" type="number" step="0.01" bind:value={sub.timeOffset} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        </div>
    </div>
    <div class="flex space-x-2">
        <input id="isDefault" type="checkbox" bind:checked={isDefault} class="mt-1 block rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        <label for="isDefault" class="block text-sm font-medium text-gray-500">Default Subtitle</label>
    </div>
    <div class="py-2 flex space-x-2 place-content-end">
        <button type="button" class="border rounded-lg px-1 text-sm border-cyan-500 text-cyan-500" on:click={doSave}>Save</button>
        <a type="button" class="border rounded-lg px-1 text-sm border-cyan-600 text-cyan-600" href="{sub.origUrl}" download>Download</a>
        <button type="button" class="border rounded-lg px-1 text-sm border-red-300 text-red-300" on:click={doDelete}>Del</button>
    </div>
</form>
{/if}
