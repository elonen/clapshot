<script lang="ts">
    import { createEventDispatcher } from 'svelte';
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import type { VideoListDefItem, VideoListVideoDef } from './types';
    import {dndzone, SHADOW_ITEM_MARKER_PROPERTY_NAME} from "svelte-dnd-action";
    import { selected_tiles } from '../../stores';

    export let id: any = {};
    export let name: string = "";
    export let contents: VideoListDefItem[] = [];

    let folder_el: HTMLElement;

    const dispatch = createEventDispatcher();
    const DUMMY_ITEM = {'id': 'dummy'};

    function contentPreviewItems(data: any[]): VideoListVideoDef[] {
        let items = data.filter(it=>("video" in it)) as VideoListVideoDef[];
        if (items.length > 4) { items = items.slice(0,4); }
        return items;
    }

let dummy_dnd_items: any = [DUMMY_ITEM];

function handleConsider(e: CustomEvent<DndEvent>) {
    // Move the shadow item to the end of the list
    dummy_dnd_items = e.detail.items
        .filter(it => !it[SHADOW_ITEM_MARKER_PROPERTY_NAME])
        .concat(e.detail.items.filter(it => it[SHADOW_ITEM_MARKER_PROPERTY_NAME]));
}

function handleFinalize(e: CustomEvent<DndEvent>) {
    console.log($selected_tiles);
    let new_items = [...e.detail.items]
        .concat(Object.keys($selected_tiles).length ? [...Object.values($selected_tiles)] : [])  // Add multiselected items
        .filter(it => (it.id && it.id !== DUMMY_ITEM.id));  // Remove shadow item
    new_items = new_items.filter((item, pos) =>             // Remove duplicates
        new_items.map((mi) => mi['id']).indexOf(item['id']) === pos );

    dispatch("drop-items-into", {'folder_id': id, 'items': new_items});
    dummy_dnd_items = [DUMMY_ITEM];
}


</script>

<div class="video-list-tile-sqr video-list-folder"
    bind:this={folder_el}
    use:dndzone="{{items: dummy_dnd_items, morphDisabled: true, dragDisabled: true, centreDraggedOnCursor: true}}"
    on:consider={handleConsider}
    on:finalize={handleFinalize}
    class:draggingOver={dummy_dnd_items.length>1}
>
    {#each dummy_dnd_items as item, i}
    {#if i == 0}
    <div class="w-full h-full flex flex-col"
    >
        <div class="flex-1 bg-[#0002] p-0.5 rounded-md shadow-inner overflow-clip leading-none text-[0px]">
            <div class="grid grid-cols-2 gap-1">
            {#each contentPreviewItems(contents) as prev, _i}
                {#if prev.video.thumb_url}
                    <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md">
                    <ScrubbableVideoThumb
                        extra_styles="border-radius: 0rem; height: 100%; width: auto; transform: translate(-50%, -50%); left: 50%; top: 50%; position: absolute; filter: opacity(66%);"
                        thumb_poster_url={prev.video.thumb_url}
                        thumb_sheet_url={prev.video.thumb_sheet_url}
                        thumb_sheet_rows={prev.video.thumb_sheet_rows}
                        thumb_sheet_cols={prev.video.thumb_sheet_cols}
                    />
                    </div>
                {/if}
            {/each}
            </div>
        </div>
        <div class="w-full flex-shrink whitespace-nowrap overflow-hidden text-xs mt-2">
            <span class="text-slate-400 text-xs font-semibold my-1">{name}</span>
        </div>
    </div>
    {:else}<span/>{/if}
    {/each}
</div>

<style>
    :global(.aboutToDrop) {
        border: 1px solid red;
        scale: 0.5;
    }

    .video-list-folder {
        background: linear-gradient(180deg, rgba(0, 0, 0, 0.2) 0%, rgba(0, 0, 0, 0.2) 100%), #3b73a5;
        clip-path: polygon(0 23%, 0 5%, 4% 0, 32% 0, 52% 7%, 60% 7%, 98% 7%, 100% 10%, 100% 100%, 15% 100%, 0 100%, 0% 85%);
        box-shadow: inset -14px -6px 32px 5px rgba(0, 0, 0, 0.2), 
                    inset 0px 12px 2px 0px rgba(0, 0, 0, 0.2);
        padding-top: 1.5em;
    }

    .draggingOver {
        transform: scale(1.075);
        transition: transform 0.1s ease-in-out;
        mix-blend-mode: screen;
        filter: brightness(1.5) !important;
    }

    :global(.activeDropTarget) .video-list-folder {
        filter: brightness(1.2);
    }
 </style>
