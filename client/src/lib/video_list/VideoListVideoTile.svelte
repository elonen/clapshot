<script lang="ts">
    import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
    import * as Proto3 from '@clapshot_protobuf/typescript';

    export let item: Proto3.Video;
    export function data() { return item; }

    function fmt_date(d: Date | undefined) {
        if (!d) return "(no date)";
        return d.toISOString().split('T')[0];
    }
</script>

<div class="w-full h-full video-list-video video-list-selector">

    {#if item.previewData?.thumbUrl}
        <ScrubbableVideoThumb
            thumb_poster_url={item.previewData?.thumbUrl}
            thumb_sheet_url={item.previewData?.thumbSheet?.url}
            thumb_sheet_rows={item.previewData?.thumbSheet?.rows}
            thumb_sheet_cols={item.previewData?.thumbSheet?.cols}
        />
    {/if}

    <div>
        <div class="w-full flex whitespace-nowrap overflow-hidden text-xs my-1">
            <span class="text-amber-400 text-xs">{fmt_date(item.addedTime)}</span>
            <span class="mx-1 text-neutral-400"> | </span>
            <span class="text-amber-500 font-mono text-xs">{item.videoHash}</span>
        </div>
        <div class="w-full video-title-line h-[3em]"><span title="{item.title}">{item.title}</span></div>
    </div>
</div>

<style>
.video-list-video {
    --tw-bg-opacity: 1;
    background-color: rgb(71 85 105 / var(--tw-bg-opacity));
    border-radius: 0.375rem;
    padding: 0.5rem;
    box-shadow: inset 0px -12px 25px 5px rgba(0, 0, 0, 0.4);
}

:global(.selectedTile .video-list-video) {
    background: rgba(241, 186, 44, 0.6);
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

