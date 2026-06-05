<script lang="ts">
import * as Proto3 from '@clapshot_protobuf/typescript';

interface Props {
    video: Proto3.MediaFile;
    onswitchversion?: (event: { mediaFileId: string }) => void;
}

let { video, onswitchversion }: Props = $props();

// Combine current video with related versions, primary (no versionOf) first
let allVersions = $derived((): Proto3.MediaFile[] => {
    const others = video.versions ?? [];
    const all = [video, ...others];
    return all.sort((a, b) => (!a.versionOf ? -1 : !b.versionOf ? 1 : 0));
});

function onSelect(e: Event) {
    const id = (e.target as HTMLSelectElement).value;
    if (id && id !== video.id && onswitchversion) {
        onswitchversion({ mediaFileId: id });
    }
}

function versionLabel(v: Proto3.MediaFile): string {
    const name = v.title || v.id;
    return v.versionOf ? name : `⭐ ${name}`;
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
        {#each allVersions() as v}
            <option value={v.id}>
                {versionLabel(v)}{v.id === video.id ? ' (actuelle)' : ''}
            </option>
        {/each}
    </select>
    <span class="text-xs text-neutral-500 whitespace-nowrap">
        {video.versions.length + 1} versions
    </span>
</div>
{/if}
