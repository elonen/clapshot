<script lang="ts">
import * as Proto3 from '@clapshot_protobuf/typescript';

interface Props {
    video: Proto3.MediaFile;
    onswitchversion?: (event: { mediaFileId: string }) => void;
}

let { video, onswitchversion }: Props = $props();

// Liste complète : version principale + versions secondaires
let allVersions = $derived(() => {
    const primary = { ...video, versions: [] };  // la principale sans sous-versions
    const secondaries = video.versions ?? [];
    return [primary, ...secondaries];
});

function onSelect(e: Event) {
    const id = (e.target as HTMLSelectElement).value;
    if (id && id !== video.id && onswitchversion) {
        onswitchversion({ mediaFileId: id });
    }
}

function versionLabel(v: Proto3.MediaFile, index: number): string {
    return v.title || v.origUrl?.split('/').pop() || `Version ${index + 1}`;
}
</script>

{#if video.versions && video.versions.length > 0}
<div class="flex items-center gap-2 px-3 py-1.5 bg-neutral-800 border-b border-neutral-700">
    <i class="fa fa-code-branch text-blue-400 text-sm"></i>
    <span class="text-xs text-neutral-400 whitespace-nowrap">Version :</span>
    <select
        class="flex-1 bg-neutral-700 text-white text-sm rounded px-2 py-1 border border-neutral-600 cursor-pointer hover:border-blue-400 focus:outline-none focus:border-blue-400"
        value={video.id}
        onchange={onSelect}
    >
        {#each allVersions() as v, i}
            <option value={v.id}>
                {i === 0 ? '⭐ ' : ''}{versionLabel(v, i)}
                {#if v.id === video.id} (actuelle){/if}
            </option>
        {/each}
    </select>
    <span class="text-xs text-neutral-500 whitespace-nowrap">
        {video.versions.length + 1} versions
    </span>
</div>
{/if}
