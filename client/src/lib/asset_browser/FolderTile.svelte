<script lang="ts">

import { createEventDispatcher } from 'svelte';
import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
import { dndzone, TRIGGERS, SOURCES } from 'svelte-dnd-action';
import { selectedTiles } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';

export let id: any = {};
export let name: string = "";
export let preview_items: Proto3.PageItem_FolderListing_Item[] = [];
export let visualization: Proto3.PageItem_FolderListing_Item_Visualization|undefined = undefined;

const dispatch = createEventDispatcher();

function contentPreviewItems(data: any[]): Proto3.PageItem_FolderListing_Item[] {
    let items = data.filter(it=>("video" in it)) as Proto3.PageItem_FolderListing_Item[];
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
// Helper function to convert RGB to CSS color
function rgbToCssColor(r:number, g:number, b:number) {
    return `rgb(${r}, ${g}, ${b})`;
}


// Convert `basecolor` (folder color override) to a CSS variable.
let basecolor = visualization?.baseColor ?
    rgbToCssColor(visualization.baseColor.r, visualization.baseColor.g, visualization.baseColor.b) :
    '#3b73a5';

function cssVariables(node: HTMLDivElement, variables: { basecolor: string; }) {
    setCssVariables(node, variables);
    return { update(variables: any) { setCssVariables(node, variables); } }
}
function setCssVariables(node: HTMLDivElement, variables: { [x: string]: any; basecolor?: string; }) {
    for (const name in variables) {
        node.style.setProperty(`--${name}`, variables[name]);
    }
}

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
        <div class="w-full flex-shrink whitespace-nowrap overflow-hidden text-s mt-2 mb-1.5">
            <span class="text-slate-400 font-semibold my-1">{name}</span>
        </div>
        {#if preview_items.length > 0}
            <div class="flex-1 bg-[#0002] p-0.5 rounded-md shadow-inner overflow-clip leading-none text-[0px]">
                <div class="grid grid-cols-2 gap-1">
                {#each contentPreviewItems(preview_items) as prev, _i}
                    {#if prev.video?.previewData?.thumbUrl }
                        <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md">
                            <ScrubbableVideoThumb
                                extra_styles="border-radius: 0rem; height: 100%; width: auto; transform: translate(-50%, -50%); left: 50%; top: 50%; position: absolute; filter: opacity(66%);"
                                thumbPosterUrl={prev.video.previewData?.thumbUrl}
                                thumbSheetUrl={prev.video.previewData?.thumbSheet?.url}
                                thumbSheetRows={prev.video.previewData?.thumbSheet?.rows}
                                thumbSheetCols={prev.video.previewData?.thumbSheet?.cols}
                            />
                        </div>
                    {:else if prev.folder }
                        <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md">
                            <div class="w-full h-full video-list-folder flex items-center justify-center">
                                <span class="text-xs text-slate-500 text-center leading-none italic">{prev.folder?.title}</span>
                            </div>
                        </div>
                    {/if}
                {/each}
                </div>
            </div>
        {:else}
            {#if visualization?.icon}
                <div class="w-full aspect-square overflow-clip inline-block relative rounded-md">
                    <div class="w-full h-full flex items-center justify-center">
                        {#if visualization.icon?.faClass}
                            <i
                                class="{visualization.icon.faClass.classes}"
                                style="color: {visualization.icon.faClass.color ? rgbToCssColor(visualization.icon.faClass.color.r, visualization.icon.faClass.color.g, visualization.icon.faClass.color.b) : '#fff'}; font-size: {visualization.icon.size || 1.5}em;"
                            ></i>
                        {:else if visualization.icon.imgUrl}
                            <img src={visualization.icon.imgUrl} class="w-1/2 h-1/2" alt="icon img"/>
                        {/if}
                    </div>
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
