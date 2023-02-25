<script lang="ts">
    import VideoListPopup from './VideoListPopup.svelte';
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import { createEventDispatcher } from 'svelte';

    const dispatch = createEventDispatcher();

    export let item: any = {};
    export let selected: boolean = false;


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

    let being_dragged = false;

    function dragStart(e: DragEvent, basketIndex: number, itemIndex: number) {
        being_dragged = true;
		const data = {basketIndex, itemIndex};
       	e.dataTransfer.setData('text/plain', JSON.stringify(data));
        console.log("video tile drag start", data)
    }

    function dragEnd(_e: DragEvent) {
        being_dragged = false;
    }
</script>

<div class="bg-slate-600 {selected?'selected':''} {being_dragged?'dragging':'?'} video-list-item w-40 h-30 relative rounded-md p-2 m-2 mx-2 overflow-clip inline-block cursor-pointer"
    tabindex="0"
    role="button"

    draggable="true"
    on:dragstart={e => dragStart(e, 42, item.video_hash /*basketIndex, itemIndex*/)}
    on:dragend={e => dragEnd(e)}

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
    <div class="w-full leading-none whitespace-nowrap overflow-hidden overflow-ellipsis text-xs"><span title="{item.title}">{item.title}</span></div>
</div>

</div>



<style>
.video-list-item {
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
    transform: scale(0.4);
    transition: transform 0.2s;
}
:global([draggable]) {
    -webkit-touch-callout:none;
    -ms-touch-action:none; touch-action:none;
    -moz-user-select:none; -webkit-user-select:none; -ms-user-select:none; user-select:none;
  }

/*  :global(div.dragged) { display:none } */

</style>

