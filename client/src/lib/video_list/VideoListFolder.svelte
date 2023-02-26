<script lang="ts">
    import VideoListPopup from './VideoListPopup.svelte';
    import { createEventDispatcher } from 'svelte';
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import type { ClapshotVideoJson } from './types';

    export let id: any = {};
    export let name: string = "";
    export let contents: ClapshotVideoJson[] = [];

    let hover_has_videos = false;
    let hover_counter = 0;

    const dispatch = createEventDispatcher();

    function onHtmlDragEnter(e: DragEvent & { currentTarget: EventTarget & HTMLDivElement; }) {
        hover_has_videos = e.dataTransfer.types.includes("clapshotvideo");
        e.preventDefault();
        e.stopPropagation();
        hover_counter++;
    }

    function onHtmlDrop(e: DragEvent & { currentTarget: EventTarget & HTMLDivElement; }) {
        hover_counter = 0;
        if (e.dataTransfer.types.includes("clapshotvideo")) {
            e.preventDefault();
            //e.stopPropagation();
            dispatch("drop", {
                folder_id: id,
                drag_data: e.dataTransfer.getData("clapshotvideo")
            });
        }
    }

    function openFolder() {
        dispatch("open-folder", {'folder_id': id});
    }

    // Show a popup menu when right-clicking on a video tile
    function onRightClick(e: MouseEvent)
    {
        let popup_container = document.querySelector('#popup-container');
        if (!popup_container) { alert("UI BUG: popup container missing"); }

        // Remove any existing popups
        for (let child of popup_container.children) { 
            if (!('hide' in child)) { alert("UI BUG: popup container child missing hide()"); }
            (child as any).hide();
        }

		new VideoListPopup({
			target: popup_container,
            props: {
                onRename: () => { dispatch("rename-folder", {'folder_id': id, 'folder_name': name}) },
                onDelete: () => { dispatch("delete-folder", {'folder_id': id, 'folder_name': name}) },
                x: e.clientX,
                y: e.clientY - 16, // Offset a bit to make it look better
            }
		});
    }

</script>

<div class="video-list-tile-sqr video-list-folder"
    class:folderhovering={hover_has_videos && hover_counter>0}
    on:dragenter={onHtmlDragEnter}
    on:dragleave={(_e) => { hover_counter--; }}
    on:drop={onHtmlDrop}

    on:contextmenu|preventDefault="{onRightClick}"
    on:click|preventDefault={openFolder}
    on:dblclick|preventDefault={openFolder}
    on:keypress={(e) => { if (e.key === 'Enter') { openFolder() }}}
>
    <div class="h-full flex flex-col">
        <div class="flex-1 bg-[#0002] p-0.5 rounded-md shadow-inner overflow-clip leading-none text-[0px]">
            <div class="grid grid-cols-2 gap-1">
            {#each contents.slice(0,4) as item, i}
                {#if item.thumb_url}
                    <div class="w-full aspect-square overflow-clip inline-block shadow-md relative rounded-md">
                    <ScrubbableVideoThumb
                        extra_styles="border-radius: 0rem; height: 100%; width: auto; transform: translate(-50%, -50%); left: 50%; top: 50%; position: absolute; filter: opacity(66%);"
                        thumb_poster_url={item.thumb_url}
                        thumb_sheet_url={item.thumb_sheet_url}
                        thumb_sheet_rows={item.thumb_sheet_rows}
                        thumb_sheet_cols={item.thumb_sheet_cols}
                    />
                    </div>
                {/if}
            {/each}
            </div>
        </div>
    
        <div class="w-full flex whitespace-nowrap overflow-hidden text-xs mt-2">
            <span class="text-slate-400 text-xs font-semibold my-1">{name}</span>
        </div>
    </div>
</div>

<style>
    .video-list-folder {
        background: linear-gradient(180deg, rgba(0, 0, 0, 0.2) 0%, rgba(0, 0, 0, 0.2) 100%), #3b73a5;
        clip-path: polygon(0 23%, 0 5%, 4% 0, 32% 0, 52% 7%, 60% 7%, 98% 7%, 100% 10%, 100% 100%, 15% 100%, 0 100%, 0% 85%); /* Folder top shape */
        box-shadow: inset -14px -6px 32px 5px rgba(0, 0, 0, 0.2), 
                    inset 0px 12px 2px 0px rgba(0, 0, 0, 0.2);
        padding-top: 1.5em;
    }

    .folderhovering {
        transform: scale(1.075);
        transition: transform 0.2s ease-in-out;
        mix-blend-mode: screen;
        filter: brightness(1.5);
    }
</style>

