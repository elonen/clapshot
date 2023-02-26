<script lang="ts">
    import VideoListPopup from './VideoListPopup.svelte';
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import { createEventDispatcher } from 'svelte';

    const dispatch = createEventDispatcher();
    let being_dragged = false;

    // --------------

    export let item: any = {};
    export let selected: boolean = false;

    export function data() { return item; }

    // Called from parent on sibling drag start.
    // Returns the video hash if this item should join the drag, or undefined if not.
    export function tryJoinVideoDrag(): string|void {
        if (selected) {
            being_dragged = true;
            return item.video_hash;
        }
    }

    // Called from parent on sibling drag end
    export function videoDragEnded() {
        being_dragged = false;
        selected = false;
    }

    // --------------

    function onDoubleClick(video_hash: string) {
        dispatch("open-video", {'video_hash': video_hash});
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
                onRename: () => { dispatch("rename-video", {'video_hash': item.video_hash, 'video_name': item.title}) },
                onDelete: () => { dispatch("delete-video", {'video_hash': item.video_hash, 'video_name': item.title}) },
                x: e.clientX,
                y: e.clientY - 16, // Offset a bit to make it look better
            }
		});
    }

    // Browser-level drag and drop events of this component's HTML elements
    function onHtmlDragStart(e: DragEvent & { currentTarget: EventTarget & HTMLDivElement; }) {
        e.dataTransfer.setData('clapshotvideo', item.video_hash);
        selected=true; 
        dispatch("drag-start", {'event': e, 'video_hash': item.video_hash})
    }
    function onHtmlDragEnd(e: DragEvent & { currentTarget: EventTarget & HTMLDivElement; }) {
        dispatch("drag-end", {'event': e, 'video_hash': item.video_hash})
        e.preventDefault();
    }
</script>

<div class="video-list-tile-sqr video-list-video"
    class:selected={selected}
    class:dragging={being_dragged}

    tabindex="0"
    role="button"

    draggable="true"
    on:dragstart={onHtmlDragStart}
    on:dragend={onHtmlDragEnd}
    
    on:contextmenu|preventDefault="{onRightClick}"
    on:click={ () => { selected = !selected; }}
    on:dblclick|preventDefault={ () => onDoubleClick(item.video_hash) }
    on:keypress={(e) => { if (e.key === 'Enter') { onDoubleClick(item.video_hash) }}}
>

    {#if item.thumb_url}
        <ScrubbableVideoThumb
            thumb_poster_url={item.thumb_url}
            thumb_sheet_url={item.thumb_sheet_url}
            thumb_sheet_rows={item.thumb_sheet_rows}
            thumb_sheet_cols={item.thumb_sheet_cols}
        />
    {/if}

    <div>
        <div class="w-full flex whitespace-nowrap overflow-hidden text-xs my-1">
            <span class="text-amber-400 text-xs">{item.added_time}</span>
            <span class="mx-1 text-neutral-400"> | </span>
            <span class="text-amber-500 font-mono text-xs">{item.video_hash}</span>
        </div>
        <div class="w-full video-title-line h-[3em]"><span title="{item.title}">{item.title}</span></div>
    </div>

</div>



<style>
.video-list-video {
  box-shadow: inset 0px -12px 25px 5px rgba(0, 0, 0, 0.4);
}
.selected {
    box-shadow: 0px 0px 0.1em 0.25em rgba(255, 200, 60, 0.8);
}
.dragging {
    opacity: 0.5;
    transition: transform 0.2s;
}
div.dragging {
    transform: scale(0.9);
    transition: transform 0.2s;
}

.video-title-line {
    font-size: 0.75rem;
    line-height: 1em;

    overflow-wrap: break-word;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;    
}

:global(.video-list-tile-sqr) {
        --tw-bg-opacity: 1;
        background-color: rgb(71 85 105 / var(--tw-bg-opacity));

        width: 10rem;
        height: 10rem;

        position: relative;
        display: inline-block;

        border-radius: 0.375rem;
        padding: 0.5rem;
        margin: 0.5rem;

        margin-left: 0.5rem/* 8px */;
        margin-right: 0.5rem/* 8px */;

        overflow: clip;
        cursor: pointer;
}

:global([draggable]) {
    -webkit-touch-callout:none;
    -ms-touch-action:none; touch-action:none;
    -moz-user-select:none; -webkit-user-select:none; -ms-user-select:none; user-select:none;
  }

</style>

