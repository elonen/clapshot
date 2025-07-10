<script lang="ts">

import { scale, slide } from "svelte/transition";
import { curSubtitle, curUserId, curUserIsAdmin, curVideo, subtitleEditingId } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';


    interface Props {
        sub: Proto3.Subtitle;
        isDefault?: boolean;
        onupdatesubtitle?: (event: {sub: Proto3.Subtitle, isDefault: boolean}) => void;
        ondeletesubtitle?: (event: {id: string}) => void;
        onchangesubtitle?: (event: {id: string}) => void;
    }

    let { sub, isDefault, onupdatesubtitle, ondeletesubtitle, onchangesubtitle }: Props = $props();

    let title = $state(sub.title);
    let languageCode = $state(sub.languageCode);
    let timeOffset = $state(sub.timeOffset);
    let isDefaultState = $state(isDefault ?? false);


function doSave() {
    const updatedSub = {...sub, title, languageCode, timeOffset};
    if (onupdatesubtitle) onupdatesubtitle({sub: updatedSub, isDefault: isDefaultState});
    $subtitleEditingId = null;
}

function doDelete() {
    if (ondeletesubtitle) ondeletesubtitle({id: sub.id});
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
        onclick={() => onchangesubtitle?.({id: sub.id})}
        ondblclick={toggleEditing}
        title={sub.origFilename}
        style="text-overflow: ellipsis; white-space: nowrap;"
    >
        <i class="fa {sub.id == $curSubtitle?.id ? 'fa-eye' : 'fa-eye-slash' }"></i>
        <span class="text-ellipsis"><strong>{sub.languageCode.toUpperCase()}</strong> – {sub.title}</span>
    </button>
    {#if $curVideo?.userId == $curUserId || $curUserIsAdmin}
    <span class="flex-shrink-0">
        <button class="fa {($subtitleEditingId==sub.id) ? "fa-angle-down" : "fa-angle-right"} hover:text-white" title="Edit subtitle" aria-label="Edit subtitle" onclick={() => { toggleEditing(); }}></button>
    </span>
    {/if}
</div>

{#if $subtitleEditingId == sub.id}
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form class="space-y-2 p-2 mb-4 rounded-lg bg-gray-800 shadow-lg shadow-black" transition:slide="{{ duration: 200 }}" onkeydown={handleKeyDown}>
    <div>
        <label for="title" class="block text-sm font-medium text-gray-500">Title</label>
        <input id="title" type="text" bind:value={title} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
    </div>
    <div class="flex space-x-2">
        <div>
            <label for="language_code" class="block text-sm font-medium text-gray-500">
                Language code
                <a href="https://en.wikipedia.org/wiki/List_of_ISO_639_language_codes" target="_blank" class="text-xs text-gray-500 hover:text-gray-300" aria-label="ISO 639 language codes information"><i class="fas fa-circle-info"></i></a>
            </label>
            <input id="language_code" minlength="2" maxlength="3"  type="text" bind:value={languageCode} class="mt-1 block w-full uppercase font-mono rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        </div>
        <div>
            <label for="time_offset" class="block text-sm font-medium text-gray-500">Time offset (sec)</label>
            <input id="time_offset" type="number" step="0.01" bind:value={timeOffset} class="mt-1 block w-full rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        </div>
    </div>
    <div class="flex space-x-2">
        <input id="isDefault" type="checkbox" bind:checked={isDefaultState} class="mt-1 block rounded-md shadow-sm text-gray-400 focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm border-gray-300">
        <label for="isDefault" class="block text-sm font-medium text-gray-500">Default Subtitle</label>
    </div>
    <div class="py-2 flex space-x-2 place-content-end">
        <button type="button" class="border rounded-lg px-1 text-sm border-cyan-500 text-cyan-500" onclick={doSave}>Save</button>
        <a type="button" class="border rounded-lg px-1 text-sm border-cyan-600 text-cyan-600" href="{sub.origUrl}" download>Download</a>
        <button type="button" class="border rounded-lg px-1 text-sm border-red-300 text-red-300" onclick={doDelete}>Del</button>
    </div>
</form>
{/if}
