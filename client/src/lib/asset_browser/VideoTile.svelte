<script lang="ts">

import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
import TileVisualizationOverride from './TileVisualizationOverride.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';
import {rgbToCssColor, cssVariables} from './utils';
import {latestProgressReports} from '@/stores';
import {slide} from "svelte/transition";
import type { MediaProgressReport } from '@/types';


    interface Props {
        item: Proto3.MediaFile;
        visualization?: Proto3.PageItem_FolderListing_Item_Visualization|undefined;
    }

    let { item, visualization = undefined }: Props = $props();

export function data() { return item; }

// Watch for (transcoding) progress reports from server, and update progress bar if one matches this item.
let progress: number|undefined = $state(undefined);
let progressMsg: string|undefined = $state(undefined);

// basecolor: gray during transcoding, otherwise derived from visualization prop
let basecolor = $derived(
    progress !== undefined ? rgbToCssColor(40, 40, 40) :
    visualization?.baseColor ?
        rgbToCssColor(visualization.baseColor.r, visualization.baseColor.g, visualization.baseColor.b) :
        rgbToCssColor(71, 85, 105)
);

$effect(() => {
    const report = $latestProgressReports?.find((r: MediaProgressReport) => r.mediaFileId === item.id);
    progress = report?.progress;
    progressMsg = report?.msg;
});

function fmt_date(d: Date | undefined) {
    if (!d) return "(no date)";
    return d.toISOString().split('T')[0];
}

</script>

<div class="w-full h-full video-list-video video-list-selector flex flex-col"
    use:cssVariables={{basecolor}}>

    <!-- Preview -->
    {#if item.previewData?.thumbUrl}
        <div class="flex-grow relative">
        <ScrubbableVideoThumb
            thumbPosterUrl={item.previewData?.thumbUrl}
            thumbSheetUrl={item.previewData?.thumbSheet?.url}
            thumbSheetRows={item.previewData?.thumbSheet?.rows}
            thumbSheetCols={item.previewData?.thumbSheet?.cols}
        />
        {#if item.versions && item.versions.length > 0}
            <div class="absolute top-1 right-1 pointer-events-none">
                <span class="inline-flex items-center gap-1 bg-blue-600 text-white text-xs font-bold px-1.5 py-0.5 rounded-full shadow">
                    <i class="fa fa-code-branch text-[9px]"></i>{item.versions.length + 1}
                </span>
            </div>
        {/if}
        </div>
    {:else if visualization}
        <div class="flex-grow relative">
        <TileVisualizationOverride vis={visualization}/>
        {#if item.versions && item.versions.length > 0}
            <div class="absolute top-1 right-1 pointer-events-none">
                <span class="inline-flex items-center gap-1 bg-blue-600 text-white text-xs font-bold px-1.5 py-0.5 rounded-full shadow">
                    <i class="fa fa-code-branch text-[9px]"></i>{item.versions.length + 1}
                </span>
            </div>
        {/if}
        </div>
    {/if}

    <!-- Progress bar (if any) -->
    {#if progress !== undefined}
        <div transition:slide class="mb-1">
            <div class="w-full text-xs font-extralight italic text-center mt-1 mb-1">{progressMsg || 'Processing...'}</div>
            <div class="w-full h-1 bg-black">
                <div class="h-full bg-amber-500" style="width: {progress * 100}%"></div>
            </div>
        </div>
    {/if}

    <!-- Metadata -->
    <div>
        <div class="w-full flex whitespace-nowrap overflow-hidden text-xs my-1">
            <span class="text-amber-400 text-xs">{fmt_date(item.addedTime)}</span>
            <span class="mx-1 text-neutral-400"> | </span>
            <span class="text-amber-500 font-mono text-xs">{item.id}</span>
        </div>
        <div class="w-full video-title-line h-[3em] mb-0"><span title="{item.title}">{item.title}</span></div>
    </div>

</div>

<style>
.video-list-video {
    --tw-bg-opacity: 1;
    background-color: var(--basecolor);
    transition: background-color 0.25s ease;
    border-radius: 0.375rem;
    padding: 0.5rem;
    box-shadow: inset 0px -12px 25px 5px rgba(0, 0, 0, 0.4);
}

:global(.selectedTile .video-list-video) {
    background: rgba(241, 186, 44, 0.6);
}

.video-title-line {
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
}
</style>
