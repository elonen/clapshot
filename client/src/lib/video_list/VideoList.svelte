    <script lang="ts">
    import {flip} from "svelte/animate";
    import {dndzone, TRIGGERS, SOURCES, SHADOW_ITEM_MARKER_PROPERTY_NAME} from "svelte-dnd-action";
    import VideoListPopup from './VideoListPopup.svelte';

    import VideoListVideoTile from "./VideoListVideoTile.svelte";
    import VideoListFolder from "./VideoListFolder.svelte";

    import type { VideoListPopupMenuItem, VideoListDefItem } from "./types";
    import { selected_tiles } from "../../stores";
    import { createEventDispatcher, tick } from "svelte";
    import { fade } from "svelte/transition";

    const dispatch = createEventDispatcher();

    export let items: VideoListDefItem[] = [];

    let dragging = false;

    function handleConsider(e: CustomEvent<DndEvent>) {
        dragging = true;
        const {items: newItems, info: {trigger, source, id}} = e.detail;
        if (source !== SOURCES.KEYBOARD) {
            if (Object.keys($selected_tiles).length && trigger === TRIGGERS.DRAG_STARTED) {
                if (Object.keys($selected_tiles).includes(id)) {
                    delete($selected_tiles[id]);
                    $selected_tiles = {...$selected_tiles};
                    tick().then(() => {
                        items = newItems.filter(item => !Object.keys($selected_tiles).includes(item.id)) as VideoListDefItem[];
                    });
                } else {
                    $selected_tiles = [];
                }
            }
        }
        if (trigger === TRIGGERS.DRAG_STOPPED) $selected_tiles = [];
        items = newItems as VideoListDefItem[];
    }
    function handleFinalize(e: CustomEvent<DndEvent>) {
        dragging = false;

        // Handle multi-selected drop
        let {items: newItems, info: {trigger, source, id}} = e.detail;
        if (Object.keys($selected_tiles).length) {
            if (trigger === TRIGGERS.DROPPED_INTO_ANOTHER) {
                items = newItems.filter(item => !Object.keys($selected_tiles).includes(item.id)) as VideoListDefItem[];
            } else if (trigger === TRIGGERS.DROPPED_INTO_ZONE || trigger === TRIGGERS.DROPPED_OUTSIDE_OF_ANY) {
                tick().then(() => {
                    const idx = newItems.findIndex(item => item.id === id);
                    // to support arrow up when keyboard dragging
                    const sidx = Math.max(Object.values($selected_tiles).findIndex(item => item.id === id), 0);
                    newItems = newItems.filter(item => !Object.keys($selected_tiles).includes(item.id)) 
                    newItems.splice(idx - sidx, 0, ...Object.values($selected_tiles));
                    items = newItems as VideoListDefItem[];
                    if (source !== SOURCES.KEYBOARD) $selected_tiles = [];
                });
            }
        } else {
            items = newItems as VideoListDefItem[];
        }
        dispatch("reorder-items", {items});
    }


    function handleMouseOrKeyDown(id: string, e: any) {
        if (dragging) {
            console.log("(dragging => videolist: ignore key/mouse down)");
            return;
        }
        // Open item by keyboard
        if (e.key) {
            if (e.key == "Enter") {
                $selected_tiles = [];
                dispatch("open-item", items.find(item => item.id === id));
                return;
            }
        }
        // (Multi-)selecting items
        if (!e.ctrlKey && !e.metaKey) return;
        if (e.key && e.key !== "Shift") return;
        if (Object.keys($selected_tiles).includes(id)) {
            delete($selected_tiles[id]);
        } else {
            $selected_tiles[id] = items.find(item => item.id === id);
        }
        $selected_tiles = {...$selected_tiles};
    }

    function transformDraggedElement(el: any) {
            if (!el.getAttribute("data-selected-items-count") && Object.keys($selected_tiles).length) {
                el.setAttribute("data-selected-items-count", Object.keys($selected_tiles).length + 1);
            }
            let style = el.querySelector(".video-list-tile-sqr").style;
            style.transition = 'all 0.2s ease-in-out';
            style.transform = "rotate(-2deg)";
            style.opacity = "0.5";
            style.scale = "0.8";
    }


    function handleMouseUp(e: MouseEvent, item: VideoListDefItem) {
        if (e.button > 0) return; // ignore right click
        if (!dragging && !e.ctrlKey) { 
            $selected_tiles = []; 
            $selected_tiles[item.id] = item; 
        }
    }

    // Show a popup menu when right-clicking on a video tile
    function onContextMenu(e: MouseEvent, item: VideoListDefItem)
    {
        let popup_container = document.querySelector('#popup-container');
        if (!popup_container) { alert("UI BUG: popup container missing"); }

        // Remove any existing popups
        for (let child of popup_container.children as any) {
            if (!('hide' in child)) { alert("UI BUG: popup container child missing hide()"); }
            child.hide();
        }

        // Which tiles are we acting on?
        let target_tiles: VideoListDefItem[] = Object.values($selected_tiles)
            .concat(item)
            .filter((item, index, self) => self.findIndex(t => t.id === item.id) === index);

        // Build the popup actions
        let popup_lines: VideoListPopupMenuItem[] = [
            {label: "Delete", action: "delete", icon_class: "fa-solid fa-trash", key_shortcut: "Del"},
        ];
        if (target_tiles.length == 1) {
            popup_lines.unshift({label: "Rename", action: "rename", icon_class: "fa-solid fa-edit", key_shortcut: "F2"});
        }

        let popup = new VideoListPopup({
            target: popup_container,
            props: {
                menu_lines: popup_lines,
                x: e.clientX,
                y: e.clientY - 16, // Offset a bit to make it look better
            },
        });
        popup.$on('action', (e) => dispatch("popup-action", {action: e.detail.action, items: target_tiles}));
        popup.$on('hide', () => popup.$destroy());

    }

    </script>


    <div>
        <section
            use:dndzone="{{
                items, transformDraggedElement,
                centreDraggedOnCursor: true,
                dropTargetClasses: ['activeDropTarget'],
                dropTargetStyle: {},
                }}"
            on:consider={handleConsider}
            on:finalize={handleFinalize}
            class="flex flex-wrap gap-4"
        >
            {#each items as item(item.id)}
                <div
                    class:selectedTile={Object.keys($selected_tiles).includes(item.id)}
                    on:click|stopPropagation
                    on:dblclick={(e) => {$selected_tiles = []; dispatch("open-item", item)}}
                    on:mousedown={(e) => handleMouseOrKeyDown(item.id, e)}
                    on:mouseup={(e) => handleMouseUp(e, item)}
                    on:keydown={(e) => handleMouseOrKeyDown(item.id, e)}
                    on:contextmenu|preventDefault={(e) => onContextMenu(e, item)}
                >
                    {#if item[SHADOW_ITEM_MARKER_PROPERTY_NAME]}
                        <div in:fade={{duration:200}} class='video-list-tile-sqr custom-dnd-shadow-item' style="height: 9.5rem;"></div>
                    {:else}
                        {#if "video" in item }
                            <VideoListVideoTile
                                item={item.video}
                                on:open-video={(_)=>{}}
                                on:delete-video={(_)=>{}}
                                on:rename-video={(_)=>{}}
                            />
                        {:else if "folder" in item }
                            <VideoListFolder
                                id={item.folder['folder_id']}
                                name={item.folder['name']}
                                contents={item.folder['contents']}
                                on:drop-items-into={(e) => {
                                    dispatch("move-to-folder", {folder_id: e.detail.folder_id, items: e.detail.items});
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

    <svelte:window on:click={(e) => {
        // Deselect all items if clicked outside of the list
        if (!dragging) $selected_tiles = []
        }} />


    <style>
        :global(.video-list-tile-sqr) {
                --tw-bg-opacity: 1;
                background-color: rgb(71 85 105 / var(--tw-bg-opacity));

                width: 10rem;
                height: 10rem;

                position: relative;
                display: block;

                border-radius: 0.375rem;
                padding: 0.5rem;

                overflow: clip;
                cursor: pointer;                
        }

        :global(.video-list-tile-sqr:hover) {
                filter: brightness(1.2);
        }

        :global(.selectedTile .video-list-tile-sqr) {
            /* box-shadow: 0 0 0.5em 0.1em rgba(241, 186, 44, 1); */
            background: rgba(241, 186, 44, 0.6);
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
        .custom-dnd-shadow-item {
            visibility: visible;
            border: 4px dashed rgb(46, 53, 69);
            background: none;
        }
    </style>