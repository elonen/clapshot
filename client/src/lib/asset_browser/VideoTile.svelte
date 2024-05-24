<script lang="ts">
import ScrubbableVideoThumb from './ScrubbableVideoThumb.svelte';
import TileVisualizationOverride from './TileVisualizationOverride.svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';
import {rgbToCssColor, cssVariables} from './utils';

export let item: Proto3.MediaFile;
export let visualization: Proto3.PageItem_FolderListing_Item_Visualization|undefined = undefined;

export function data() { return item; }

function fmt_date(d: Date | undefined) {
    if (!d) return "(no date)";
    return d.toISOString().split('T')[0];
}

// Convert `basecolor` (folder color override) to a CSS variable.
let basecolor = visualization?.baseColor ?
    rgbToCssColor(visualization.baseColor.r, visualization.baseColor.g, visualization.baseColor.b) :
    rgbToCssColor(71, 85, 105);

</script>

<div class="w-full h-full video-list-video video-list-selector flex flex-col"
    use:cssVariables={{basecolor}}>

    <!-- Preview -->
    {#if item.previewData?.thumbUrl}
        <div class="flex-grow">
        <ScrubbableVideoThumb
            thumbPosterUrl={item.previewData?.thumbUrl}
            thumbSheetUrl={item.previewData?.thumbSheet?.url}
            thumbSheetRows={item.previewData?.thumbSheet?.rows}
            thumbSheetCols={item.previewData?.thumbSheet?.cols}
        />
        </div>
    {:else if visualization }
        <div class="flex-grow">
        <TileVisualizationOverride vis={visualization}/>
        </div>
    {/if}

    <!-- Metadata -->
    <div>
        <div class="w-full flex whitespace-nowrap overflow-hidden text-xs my-1">
            <span class="text-amber-400 text-xs">{fmt_date(item.addedTime)}</span>
            <span class="mx-1 text-neutral-400"> | </span>
            <span class="text-amber-500 font-mono text-xs">{item.id}</span>
        </div>
        <div class="w-full video-title-line h-[3em]"><span title="{item.title}">{item.title}</span></div>
    </div>

</div>

<style>
.video-list-video {
    --tw-bg-opacity: 1;
    background-color: var(--basecolor); /* rgb(71 85 105 / var(--tw-bg-opacity)); */
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

