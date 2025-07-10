<script lang="ts">
    import { createBubbler, stopPropagation } from 'svelte/legacy';

    const bubble = createBubbler();

import {dndzone, TRIGGERS, SOURCES, SHADOW_ITEM_MARKER_PROPERTY_NAME, type DndEvent} from "svelte-dnd-action";
import PopupMenu from './PopupMenu.svelte';
import VideoTile from "./VideoTile.svelte";
import FolderTile from "./FolderTile.svelte";

import { folderItemsToIDs, type VideoListDefItem } from "./types";
import { createEventDispatcher, tick, mount, unmount } from "svelte";
import { fade } from "svelte/transition";

import { selectedTiles, serverDefinedActions } from "@/stores";
import * as Proto3 from '@clapshot_protobuf/typescript';

  /*
   * ⚠️  COMPLEXITY WARNING ⚠️
   *
   * This component integrates svelte-dnd-action library with complex selection, keyboard navigation,
   * and drag-and-drop functionality. Multiple intricate interdependencies exist that are NOT obvious
   * from reading the code. Proceed with caution when making changes.
   *
   * 1. **svelte-dnd-action Keyboard Event Interception**:
   *    - The library intercepts Enter/Space keys at a very low level (capture phase or earlier)
   *    - These keys are automatically converted to drag operations (dragStarted/dragStopped events)
   *    - NO configuration exists to exclude specific keys from drag activation
   *    - Solution: Custom `enterKeyInterceptor` action uses capture-phase listeners to intercept
   *      Enter before the library gets it, while leaving Space for drag operations
   *
   * 2. **Event Handler Hierarchy Complexity**:
   *    - handleMouseDown: Library intercepts these events, selection logic CANNOT go here
   *    - handleMouseUp: Where selection logic actually works
   *    - handleKeyDown: Enter events never reach here due to library interception
   *    - The event flow is: capture phase → dnd library → bubble phase → our handlers
   *
   * 3. **isDragging State Management**:
   *    - CRITICAL: Only set to true on TRIGGERS.DRAG_STARTED, not every handleConsider event
   *    - An old version of this code set it true for all consider events, breaking keyboard navigation
   *    - This state affects selection behavior, keyboard handling, and visual feedback
   *
   * 4. **Zone Type Interactions**:
   *    - NEVER add custom `type` to dndzone configuration
   *    - FolderTile components use default type ('Internal')
   *    - Different types cannot interact, breaking drop-into-folder functionality
   *    - This was a subtle breaking change that took significant debugging to identify
   *
   * 5. **Focus vs Selection State**:
   *    - Visual selection ($selectedTiles) vs keyboard focus are separate concerns
   *    - Enter key only works on keyboard-focused elements, not just visually selected ones
   *    - Solution: Manually set focus() on mouse click for consistent behavior
   *    - Custom focus styling prevents harsh white outlines while maintaining functionality
   *
   * 6. **Multi-Select Implementation**:
   *    - Shift+click: Range selection using getItemsInRange() and lastSelectedItemId tracking
   *    - Cmd/Ctrl+click: Individual item toggles
   *    - Range selection works across flexbox layout by using item array indices
   *    - MUST maintain lastSelectedItemId correctly to avoid broken range selections
   *
   * 7. **Nested DnD Zone Dependencies**:
   *    - Main list: One dndzone for reordering
   *    - Folder tiles: Individual dndzone for accepting drops
   *    - Drop-into-folder requires BOTH zones to work together correctly
   *    - Event flow between nested zones is fragile and easily broken
   *
   * 8. **Multi-Select Drag Emulation**:
   *    - Library doesn't natively support multi-item drag
   *    - Custom logic in handleConsider/handleFinalize emulates this behavior
   *    - selectedTiles store tracks multiple items for drag operations
   *    - Extremely complex interaction between selection state and drag events
   *
   * BEFORE MAKING CHANGES:
   * - Test ALL interaction modes: mouse click, drag, keyboard nav, multi-select, range select
   * - Test drop-into-folder functionality specifically
   * - Test Enter key opening items vs Space key drag operations
   * - Verify focus behavior matches visual selection
   * - Check that nested folder tiles still receive drag events properly
   *
   * DANGEROUS CHANGES TO AVOID:
   * - Adding/changing dndzone `type` configuration
   * - Moving selection logic to different event handlers
   * - Modifying isDragging state management
   * - Changing how enterKeyInterceptor works
   * - Altering the nested dndzone structure
   *
   * This complexity exists because we're forcing the library to support use cases it wasn't
   * designed for (multi-select + drag, Enter key opening, nested drop zones). The current
   * implementation works but is inherently fragile.
   */

const dispatch = createEventDispatcher();

    interface Props {
        listingData: { [key: string]: string };
        items?: VideoListDefItem[];
        dragDisabled?: boolean;
        listPopupActions?: string[];
    }

    let {
        listingData,
        items = $bindable([]),
        dragDisabled = true,
        listPopupActions = []
    }: Props = $props();

let isDragging = $state(false);
let lastSelectedItemId = $state<string | null>(null);

function mapDefItems(items: VideoListDefItem[]) {
    return folderItemsToIDs(items.map((it)=>it.obj));
}

function getItemsInRange(startId: string, endId: string): VideoListDefItem[] {
    const startIndex = items.findIndex(item => item.id === startId);
    const endIndex = items.findIndex(item => item.id === endId);

    if (startIndex === -1 || endIndex === -1) return [];

    const minIndex = Math.min(startIndex, endIndex);
    const maxIndex = Math.max(startIndex, endIndex);

    return items.slice(minIndex, maxIndex + 1);
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

    if (trigger === TRIGGERS.DRAG_STOPPED) {
        isDragging = false;
        $selectedTiles = {};
    }
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

function handleMouseDown(id: string, e: MouseEvent) {
    if (isDragging) {
        console.log("(dragging => videolist: ignore mouse down)");
        return;
    }
    hidePopupMenus();

    const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

    // Handle Ctrl+click on macOS as context menu (equivalent to right-click)
    if (isMac && e.ctrlKey) {
        e.preventDefault();
        e.stopPropagation();
        let item = items.find(item => item.id === id);
        if (item) {
            onContextMenu(e, item);
        }
        return;
    }

    // Note: Selection logic moved to handleMouseUp since mousedown events are intercepted by drag library
}

function handleKeyDown(id: string, e: KeyboardEvent) {
    // Note: Enter key is handled by enterKeyInterceptor action
    if (isDragging) {
        return;
    }
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

    if (!isDragging) {
        const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
        const isMultiSelectKey = isMac ? e.metaKey : e.ctrlKey;

        if (e.shiftKey && lastSelectedItemId) {
            // Range selection: select all items between lastSelectedItemId and current item
            const rangeItems = getItemsInRange(lastSelectedItemId, item.id);
            for (const rangeItem of rangeItems) {
                $selectedTiles[rangeItem.id] = rangeItem;
            }
            $selectedTiles = {...$selectedTiles};
            // Don't update lastSelectedItemId for range selection
        } else if (isMultiSelectKey) {
            // Toggle individual item
            if (Object.keys($selectedTiles).includes(item.id)) {
                delete($selectedTiles[item.id]);
            } else {
                $selectedTiles[item.id] = item;
            }
            $selectedTiles = {...$selectedTiles};
            lastSelectedItemId = item.id;
        } else {
            // Single selection
            $selectedTiles = {};
            $selectedTiles[item.id] = item;
            lastSelectedItemId = item.id;

            // Set keyboard focus to match visual selection
            const element = document.getElementById(`videolist_item__${item.id}`);
            if (element) {
                element.focus();
            }
        }
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

    let popup = mount(PopupMenu, {
            target: popupContainer ,
            props: {
                menuLines: actions,
                x: e.clientX,
                y: e.clientY - 16, // Offset a bit to make it look better
                onaction: (event: any) => dispatch("popup-action", {action: event.action, items: targetTiles, listingData}),
                onhide: () => unmount(popup)
            },
        });
}

function isShadowItem(item: any) {
    return item[SHADOW_ITEM_MARKER_PROPERTY_NAME];
}

// Custom action to intercept Enter key before dnd library gets it
function enterKeyInterceptor(node: HTMLElement) {
    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && !event.repeat) {
            const focusedElement = document.activeElement;
            if (focusedElement && focusedElement.id.startsWith("videolist_item__")) {
                console.log("### INTERCEPTED ENTER WITH CAPTURE PHASE");
                event.preventDefault();
                event.stopPropagation();
                event.stopImmediatePropagation();

                const itemId = focusedElement.id.replace("videolist_item__", "");
                dispatchOpenItem(itemId);
                $selectedTiles = {};
            }
        }
    }

    // Capture phase runs before svelte-dnd-action's handlers
    node.addEventListener('keydown', handleKeydown, { capture: true });

    return {
        destroy() {
            node.removeEventListener('keydown', handleKeydown, { capture: true });
        }
    };
}
</script>

<div use:enterKeyInterceptor>
    <section
        use:dndzone="{{
            items, dragDisabled,
            transformDraggedElement,
            centreDraggedOnCursor: true,
            dropTargetClasses: ['activeDropTarget'],
            dropTargetStyle: {},
            }}"
        onconsider={handleConsider}
        onfinalize={handleFinalize}
        oncontextmenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onContextMenu(e, null);
        }}
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
                onclick={stopPropagation(bubble('click'))}
                ondblclick={(_e) => {$selectedTiles = {}; dispatchOpenItem(item.id)}}
                onmousedown={(e) => handleMouseDown(item.id, e)}
                onmouseup={(e) => handleMouseUp(e, item)}
                onkeydown={(e) => handleKeyDown(item.id, e)}
                oncontextmenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onContextMenu(e as MouseEvent, item);
                }}
            >
                {#if isShadowItem(item)}
                    <div in:fade={{duration:200}} class='custom-dnd-shadow-item'></div>
                {:else}
                    {#if item.obj.mediaFile}
                        <VideoTile item={item.obj.mediaFile} visualization={item.obj.vis}/>
                    {:else if item.obj.folder}
                        <FolderTile
                            id={item.obj.folder.id}
                            name={item.obj.folder.title}
                            preview_items={item.obj.folder.previewItems }
                            visualization={item.obj.vis}
                            ondropitemsinto={(event) => {
                                dispatch("move-to-folder", {
                                    dstFolderId: event.folderId,
                                    ids: mapDefItems(event.items) });
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

<svelte:window onclick={(_e) => {
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

:global(.video-list-tile-sqr:focus .video-list-video),
:global(.video-list-tile-sqr:focus .video-list-folder) {
    background: rgba(251, 200, 60, 0.8) !important;
}

:global(.video-list-tile-sqr:focus) {
    outline: none;
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
