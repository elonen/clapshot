<script lang="ts">
    import VideoListPopup from './VideoListPopup.svelte';
    import { createEventDispatcher } from 'svelte';

    const dispatch = createEventDispatcher();

    export let item: any = {};
    export let selected: boolean = false;

    let popup_menu: any = null;

    function installThumbScrubber(e: MouseEvent, item: object)
    {
        let sheet_cols = item.thumb_sheet_cols;
        let sheet_rows = item.thumb_sheet_rows;
        let bgImg =  new Image();
        
        bgImg.onload = (le) => {
            // Total size of sprite sheet in pixels
            let sheet_w_px = le.target.naturalWidth;
            let sheet_h_px = le.target.naturalHeight;

            // Size of one frame in pixels
            let frame_width = sheet_w_px / sheet_cols;
            let frame_height = sheet_h_px / sheet_rows;

            // Size of current div (that shows the sprite sheet) in pixels
            let div_w_px = e.target.clientWidth;
            let div_h_px = e.target.clientHeight;

            // Switch background image to the now loaded sprite sheet
            e.target.style.backgroundRepeat = 'no-repeat';
            e.target.style.backgroundImage = 'url(' + bgImg.src + ')';

            // Scale the sprite sheet so one frame fits in the div
            let scaled_bgr_w = (div_w_px / frame_width) * sheet_w_px;
            let scaled_bgr_h = (div_h_px / frame_height) * sheet_h_px;
            e.target.style.backgroundSize = scaled_bgr_w + 'px ' + scaled_bgr_h + 'px';

            function show_frame(frame_idx) {
                let frame_xi = frame_idx % sheet_cols;
                let frame_yi = Math.floor(frame_idx / sheet_cols);

                let frame_left = scaled_bgr_w * (frame_xi / sheet_cols);
                let frame_top = scaled_bgr_h * (frame_yi / sheet_rows);

                e.target.style.backgroundPosition = '-' + frame_left + 'px -' + frame_top + 'px';
            }

            // Show first frame at first
            show_frame(0);

            // Scrub sheet on mouse move
            e.target.onmousemove = (e) => {
                let frame_idx = Math.floor((e.offsetX / e.target.clientWidth) * (sheet_cols * sheet_rows));
                show_frame(frame_idx);
            }
        };
        // Start loading the sprite sheet
        bgImg.src = item.thumb_sheet_url;
    }

    function removeThumbScrubber(e: MouseEvent, item: object)
    {
        // Restore original background image (item.thumb_url)
        e.target.onmousemove = null;
        e.target.onload = null;
        e.target.style.backgroundImage = 'url(' + item.thumb_url + ')';
        e.target.style.backgroundPosition = '0 0';
        e.target.style.backgroundSize = '100% 100%';
    }

    function onDoubleClick(video_hash) {
        dispatch("open-video", {'video_hash': video_hash});
    }

    let showMenu = false;
    let pos = { x: 0, y: 0 };

    async function onBodyContextMenu(e: MouseEvent) {
        return true;

		if (showMenu) {
			showMenu = false;
            popup_menu.hide();
			await new Promise(res => setTimeout(res, 100));
		}
		pos = { x: e.clientX, y: e.clientY };
		showMenu = true;
		console.log("pos:", pos)
        popup_menu.show(e);
	}

    function onRightClick(e: MouseEvent) {
        console.log("onRightClick", popup_menu)
        popup_menu.show(e);
    }


    let being_dragged = false;

    function dragStart(e: DragEvent, basketIndex: number, itemIndex: number) {
        being_dragged = true;
		const data = {basketIndex, itemIndex};
       	e.dataTransfer.setData('text/plain', JSON.stringify(data));
        console.log("video tile drag start", data)
    }

    function dragEnd(e: DragEvent, basketIndex: number) {
        being_dragged = false;
    }


</script>

<VideoListPopup
bind:this={popup_menu}
onDelete={() => { dispatch("delete-video", {'video_hash': item.video_hash, 'video_name': item.title}) }}
onRename={() => { dispatch("rename-video", {'video_hash': item.video_hash, 'video_name': item.title}) }}
/>

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
<!-- hover mouse to scrub thumb sheet -->
<div class="w-full aspect-video mx-auto rounded-md overflow-hidden"
  style="background-image: url('{item.thumb_url}'); background-size: cover; background-position: 0 0;"
  on:blur={()=>{}}
  on:focus={()=>{}}
  on:mouseover={(e) => installThumbScrubber(e, item)}
  on:mouseout={(e) => removeThumbScrubber(e, item)}
>
</div>
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

<svelte:body on:contextmenu={onBodyContextMenu} />




<style>
.video-list-item {
  box-shadow: inset 0px -12px 25px 5px rgba(0, 0, 0, 0.4);
}
.selected {
    box-shadow: 0px 0px 0.1em 0.25em rgba(255, 200, 60, 0.8);
}
.dragging {
    /* box-shadow: 0px 0px 0.1em 0.25em rgba(255, 0, 0, 0.8); */
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

