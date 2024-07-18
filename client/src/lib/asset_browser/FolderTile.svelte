<script lang="ts">

import { createEventDispatcher } from 'svelte';
import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
import { dndzone, TRIGGERS, SOURCES } from 'svelte-dnd-action';
import { selectedTiles } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';
import TileVisualizationOverride from './TileVisualizationOverride.svelte';
import {rgbToCssColor, cssVariables} from './utils';

export let id: any = {};
export let name: string = "";
export let preview_items: Proto3.PageItem_FolderListing_Item[] = [];
export let visualization: Proto3.PageItem_FolderListing_Item_Visualization|undefined = undefined;

const dispatch = createEventDispatcher();

function contentPreviewItems(data: Proto3.PageItem_FolderListing_Item[]): Proto3.PageItem_FolderListing_Item[] {
    let items = data;
    if (items.length > 4) { items = items.slice(0,4); }
    return items;
}

// This only holds a DnD item temporarily during keyboard DnD, and shadow items
// when reordering by pointer.
let dndItems: any = [];

function onSink(e: any) {
	console.log("Sunk #" + e.detail.items[0].id + " into #" + id)

    // Add multiselected items
    let newItems = [...e.detail.items].concat(
        Object.keys($selectedTiles).length ? [...Object.values($selectedTiles)] : []);
    // Remove duplicates
    newItems = newItems.filter((item, pos) =>
        newItems.map((mi) => mi['id']).indexOf(item['id']) === pos );

    dispatch("drop-items-into", {'folderId': id, 'items': newItems});

    dndItems = [];
}

function consider(e: any) {
	if (e.detail.info.trigger == TRIGGERS.DRAG_STOPPED &&
		  e.detail.info.source == SOURCES.KEYBOARD) {
		// On keyboard drag, DRAG_STOPPED on consider() is the _real_ "finalize" state
		onSink(e);
	} else {
		dndItems = e.detail.items;
	}
}

function finalize(e: any) {
	if (e.detail.info.source == SOURCES.KEYBOARD) {
		// On keyboard, dragged item can be taken back out by another (shift-)tab key hit,
		// so we have to keep in `items` for now:
		dndItems = e.detail.items;
	} else {
		// On pointer, finalize() is actually final. Sink the item.
		onSink(e);
	}
}

// Convert `basecolor` (folder color override) to a CSS variable.
let basecolor = visualization?.baseColor ?
    rgbToCssColor(visualization.baseColor.r, visualization.baseColor.g, visualization.baseColor.b) :
    '#3b73a5';

</script>


<div class="w-full h-full video-list-selector"
    style="position: relative;"
    class:draggingOver={dndItems.length>0}
    use:cssVariables={{basecolor}}
>

    <div class="w-full h-full video-list-folder"
        use:dndzone="{{items: dndItems, morphDisabled: true, dragDisabled: true, zoneTabIndex: -1, centreDraggedOnCursor: true}}"
        on:consider={consider}
        on:finalize={finalize}
    >
    {#each dndItems as _item, _i}<span/>{/each}
    </div>

    <div class="w-[85%] h-[85%] flex flex-col folder-deco" title="{name}">
        <div class="w-full flex-shrink-0 min-h-[1em] whitespace-nowrap overflow-hidden text-s mt-1 mb-0.5">
            <div class="text-slate-400 w-full video-title-line py-0 my-0"><span title="{name}">{name}</span></div>
        </div>
        {#if preview_items.length > 0}
        <div class="flex-1 bg-[#0002] p-0.5 rounded-md shadow-inner overflow-clip leading-none text-[0px]">
            <div class="grid grid-cols-2 gap-1 text-xs max-h-4">
                {#each contentPreviewItems(preview_items) as prev, _i}
                    {#if prev.mediaFile?.previewData?.thumbUrl }
                        <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md" style="background: rgb(71, 85, 105)">
                            <ScrubbableVideoThumb
                                extra_styles="border-radius: 0rem; height: 100%; width: auto; transform: translate(-50%, -50%); left: 50%; top: 50%; position: absolute; filter: opacity(66%);"
                                thumbPosterUrl={prev.mediaFile.previewData?.thumbUrl}
                                thumbSheetUrl={prev.mediaFile.previewData?.thumbSheet?.url}
                                thumbSheetRows={prev.mediaFile.previewData?.thumbSheet?.rows}
                                thumbSheetCols={prev.mediaFile.previewData?.thumbSheet?.cols}
                            />
                        </div>
                    {:else if prev.folder }
                        <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md">
                            <div class="w-full h-full video-list-folder flex items-center justify-center">
                                <span class="text-xs text-slate-500 text-center leading-none italic">{prev.folder?.title}</span>
                            </div>
                        </div>
                    {:else if prev.vis }
                        <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md" style="background: rgb(71, 85, 105)">
                            <TileVisualizationOverride
                                extra_styles="filter: opacity(66%);"
                                vis={prev.vis}/>
                        </div>
                    {/if}
                {/each}
            </div>
        </div>
        {:else}
            {#if visualization}
                <div class="w-full aspect-square overflow-clip inline-block relative rounded-md">
                    <TileVisualizationOverride vis={visualization}/>
                </div>
            {/if}
        {/if}
    </div>

</div>

<style>
:global(.aboutToDrop) {
    border: 1px solid red;
    scale: 0.5;
}

.folder-deco {
    position: absolute; left:50%; top:50%;
    transform: translate(-50%, -50%);
}
.video-list-folder {
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.2) 0%, rgba(0, 0, 0, 0.2) 100%), var(--basecolor);
    clip-path: polygon(0 23%, 0 5%, 4% 0, 32% 0, 52% 7%, 60% 7%, 98% 7%, 100% 10%, 100% 100%, 15% 100%, 0 100%, 0% 85%);
    box-shadow: inset -14px -6px 32px 5px rgba(0, 0, 0, 0.2),
                inset 0px 12px 2px 0px rgba(0, 0, 0, 0.2);
    padding-top: 1.5em;
    border-radius: 0.375rem;
}

:global(.selectedTile .video-list-folder) {
    background: rgba(241, 186, 44, 0.6);
}

:global(.activeDropTarget) .video-list-folder {
    filter: brightness(1.2);
}
</style>
