<script lang="ts">
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import { createEventDispatcher } from 'svelte';

    export let item: any = {};

    export function data() { return item; }
</script>

<div class="video-list-tile-sqr video-list-video">

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

</style>

