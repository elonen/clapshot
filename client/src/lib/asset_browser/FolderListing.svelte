<script lang="ts">

import {dndzone, TRIGGERS, SOURCES, SHADOW_ITEM_MARKER_PROPERTY_NAME} from "svelte-dnd-action";
import PopupMenu from './PopupMenu.svelte';
import VideoTile from "./VideoTile.svelte";
import FolderTile from "./FolderTile.svelte";

import { folderItemsToIDs, type VideoListDefItem } from "./types";
import { createEventDispatcher, tick } from "svelte";
import { fade } from "svelte/transition";

import { selectedTiles, serverDefinedActions } from "@/stores";
import * as Proto3 from '@clapshot_protobuf/typescript';

const dispatch = createEventDispatcher();

export let listingData: { [key: string]: string };
export let items: VideoListDefItem[] = [];
export let dragDisabled: boolean = true;
export let listPopupActions: string[] = [];

let isDragging = false;

function mapDefItems(items: VideoListDefItem[]) {
    return folderItemsToIDs(items.map((it)=>it.obj));
}

function handleConsider(e: CustomEvent<DndEvent>) {
    isDragging = true;
    const {items: newItems, info: {trigger, source, id}} = e.detail;
    if (source !== SOURCES.KEYBOARD) {
        if (Object.keys($selectedTiles).length && trigger === TRIGGERS.DRAG_STARTED) {
            if (Object.keys($selectedTiles).includes(id)) {
                delete($selectedTiles[id]);
                $selectedTiles = {...$selectedTiles};
                tick().then(() => {
                    items = newItems.filter(item => !Object.keys($selectedTiles).includes(item.id)) as VideoListDefItem[];
                });
            } else {
                $selectedTiles = {};
            }
        }
    }
    if (trigger === TRIGGERS.DRAG_STOPPED) $selectedTiles = {};
    items = newItems as VideoListDefItem[];
}
function handleFinalize(e: CustomEvent<DndEvent>) {
    isDragging = false;

    // Handle multi-selected drop
    let {items: newItems, info: {trigger, source, id}} = e.detail;
    if (Object.keys($selectedTiles).length) {
        if (trigger === TRIGGERS.DROPPED_INTO_ANOTHER) {
            items = newItems.filter(item => !Object.keys($selectedTiles).includes(item.id)) as VideoListDefItem[];
        } else if (trigger === TRIGGERS.DROPPED_INTO_ZONE || trigger === TRIGGERS.DROPPED_OUTSIDE_OF_ANY) {
            tick().then(() => {
                const idx = newItems.findIndex(item => item.id === id);
                // to support arrow up when keyboard dragging
                const sidx = Math.max(Object.values($selectedTiles).findIndex(item => item.id === id), 0);
                newItems = newItems.filter(item => !Object.keys($selectedTiles).includes(item.id))
                newItems.splice(idx - sidx, 0, ...Object.values($selectedTiles));
                items = newItems as VideoListDefItem[];
                if (source !== SOURCES.KEYBOARD) $selectedTiles = {};
            });
        }
    } else {
        items = newItems as VideoListDefItem[];
    }
    dispatch("reorder-items", { listingData, ids: mapDefItems(items) });
}

function dispatchOpenItem(id: string) {
    let it = items.find(item => item.id === id);
    if (it && it.obj.openAction) {
        let el = document.getElementById("videolist_item__" + id);
        if (!el) { alert("UI BUG: item not found"); } else {
            el.classList.add("videolist_item_pump_anim");
            setTimeout(() => { el?.classList.remove("videolist_item_pump_anim"); }, 1000);
        }
        dispatch("open-item", { item: it.obj, listingData });
    } else {
        alert("UI BUG: item not found or missing openAction");
    }
}

function handleMouseOrKeyDown(id: string, e: any) {
    if (isDragging) {
        console.log("(dragging => videolist: ignore key/mouse down)");
        return;
    }
    hidePopupMenus();

    // Open item by keyboard
    if (e.key) {
        if (e.key == "Enter") {
            dispatchOpenItem(id);
            $selectedTiles = {};
            return;
        }
    }
    // (Multi-)selecting items
    if (!e.shiftKey ) return;
    if (e.key && e.key !== "Shift") return;
    if (Object.keys($selectedTiles).includes(id)) {
        delete($selectedTiles[id]);
    } else {
        let it = items.find(item => item.id === id);
        if (it)
            $selectedTiles[id] = it;
        else
            console.error("UI BUG: videolist item not found");
    }
    $selectedTiles = {...$selectedTiles};
}

function transformDraggedElement(el: any) {
    if (!el.getAttribute("data-selected-items-count") && Object.keys($selectedTiles).length) {
        el.setAttribute("data-selected-items-count", Object.keys($selectedTiles).length + 1);
    }
    let style = el.querySelector(".video-list-selector").style;
    style.transition = 'all 0.2s ease-in-out';
    style.transform = "rotate(-2deg)";
    style.opacity = "0.5";
    style.scale = "0.8";
}


function handleMouseUp(e: MouseEvent, item: VideoListDefItem) {
    if (e.button > 0) return; // ignore right click
    if (!isDragging && !e.shiftKey) {
        $selectedTiles = {};
        $selectedTiles[item.id] = item;
    }
}

function hidePopupMenus() {
    let popupContainer = document.querySelector('#popup-container');
    if (!popupContainer) { alert("UI BUG: popup container missing"); return; }
    for (let child of popupContainer.children as any) {
        if (!('hide' in child)) { alert("UI BUG: popup container child missing hide()"); }
        child.hide();
    }
}

// Show a popup menu when right-clicking on a video tile
function onContextMenu(e: MouseEvent, item: VideoListDefItem|null)
{
    let popupContainer = document.querySelector('#popup-container');
    if (!popupContainer) { alert("UI BUG: popup container missing"); return; }
    hidePopupMenus();

    let actions: Proto3.ActionDef[] = [];
    let targetTiles: VideoListDefItem[] = [];
    if (item)
    {
        // Which tiles are we acting on?
        targetTiles = Object.values($selectedTiles)
            .concat(item)
            .filter((item, index, self) => self.findIndex(t => t.id === item.id) === index); // unique

        // Build the popup menu items (actions)
        actions = targetTiles.map(tile => tile.obj.popupActions).flat()
            .filter((actionId, index, self) => self.indexOf(actionId) === index)  // unique action ids
            .map(aid => {   // convert ids to action objects
                    let a = $serverDefinedActions[aid];
                    if (!a) { alert("UI / Organizer BUG: popup action '" + aid + "' not found"); }
                    return a;
                })
            .filter(a => a !== undefined);
    }
    else
    {
        // No item => user right-clicked on empty space in the list
        actions = listPopupActions.map(aid => {
                let a = $serverDefinedActions[aid];
                if (!a) { alert("UI / Organizer BUG: popup action '" + aid + "' not found"); }
                return a;
            });
    }

    if (actions.length === 0)
        return;

    let popup = new PopupMenu({
        target: popupContainer ,
        props: {
            menuLines: actions,
            x: e.clientX,
            y: e.clientY - 16, // Offset a bit to make it look better
        },
    });
    popup.$on('action', (e) => dispatch("popup-action", {action: e.detail.action, items: targetTiles, listingData}));
    popup.$on('hide', () => popup.$destroy());
    e.preventDefault(); // Prevent default browser context menu
}

function isShadowItem(item: any) {
    return item[SHADOW_ITEM_MARKER_PROPERTY_NAME];
}
</script>

<div>
    <section
        use:dndzone="{{
            items, dragDisabled,
            transformDraggedElement,
            centreDraggedOnCursor: true,
            dropTargetClasses: ['activeDropTarget'],
            dropTargetStyle: {},
            }}"
        on:consider={handleConsider}
        on:finalize={handleFinalize}
        on:contextmenu={(e) => onContextMenu(e, null)}
        class="flex flex-wrap gap-4 p-4 bg-slate-900"
        role="list"
    >
        {#each items as item(item.id)}
            <div
                id="videolist_item__{item.id}"
                class="video-list-tile-sqr"
                role="button"
                tabindex="0"
                class:selectedTile={Object.keys($selectedTiles).includes(item.id)}
                on:click|stopPropagation
                on:dblclick={(_e) => {$selectedTiles = {}; dispatchOpenItem(item.id)}}
                on:mousedown={(e) => handleMouseOrKeyDown(item.id, e)}
                on:mouseup={(e) => handleMouseUp(e, item)}
                on:keydown={(e) => handleMouseOrKeyDown(item.id, e)}
                on:contextmenu|stopPropagation={(e) => onContextMenu(e, item)}
            >
                {#if isShadowItem(item)}
                    <div in:fade={{duration:200}} class='custom-dnd-shadow-item'></div>
                {:else}
                    {#if item.obj.video }
                        <VideoTile item={item.obj.video} />
                    {:else if item.obj.folder }
                        <FolderTile
                            id={item.obj.folder.id}
                            name={item.obj.folder.title}
                            preview_items={item.obj.folder.previewItems }
                            visualization={item.obj.vis}
                            on:drop-items-into={(e) => {
                                dispatch("move-to-folder", {
                                    dstFolderId: e.detail.folderId,
                                    ids: mapDefItems(e.detail.items) });
                            }}
                        />
                    {:else}
                        <div>Unknown item type</div>
                    {/if}
                {/if}
            </div>
        {/each}
    </section>
</div>

<svelte:window on:click={(_e) => {
    // Deselect all items if clicked outside of the list
    if (!isDragging) $selectedTiles = {};
}} />


<style>
:global(.video-list-tile-sqr) {
    width: 10rem;
    height: 10rem;

    position: relative;
    display: block;

    overflow: clip;
    cursor: pointer;
}

:global(.video-list-tile-sqr:hover) {
        filter: brightness(1.2);
}

:global([data-selected-items-count]::after) {
    /* Show count of selected items in the corner */
    position: absolute;
    right: 0.2em;
    content: attr(data-selected-items-count);
    font-size: x-large;
    color: white;
    padding: 0.5em;
    background: rgba(174, 134, 33, 0.8);
    border-radius: 1em;
    box-shadow: 0 0 0.5em rgba(0, 0, 0, 0.8);
    border: 0.1em solid rgba(0, 0, 0, 0.8);
}

:global(.video-list-tile-sqr:has(.draggingOver)) {
    transform: scale(1.075);
    transition: transform 0.1s ease-in-out;
    mix-blend-mode: screen;
    filter: brightness(1.5) !important;
}

.custom-dnd-shadow-item {
    height: 100%;
    width: 100%;
    border-radius: 0.8rem;
    visibility: visible;
    border: 4px dashed rgb(46, 53, 69);
    background: none;
}
</style>
