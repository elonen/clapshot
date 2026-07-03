<script lang="ts">
import * as Proto3 from '@clapshot_protobuf/typescript';
import { rgbToCssColor } from './utils';

    interface Props {
        badges?: Proto3.PageItem_FolderListing_Item_Visualization_Badge[];
    }

    let { badges = [] }: Props = $props();

const DEFAULT_COLOR = "rgb(234, 88, 12)";  // orange

function badgeColor(b: Proto3.PageItem_FolderListing_Item_Visualization_Badge): string {
    return b.color ? rgbToCssColor(b.color.r, b.color.g, b.color.b) : DEFAULT_COLOR;
}
</script>

{#if badges && badges.length > 0}
<div class="corner-badges" data-testid="badges">
    {#each badges as b}
        <span class="corner-badge" style="background-color: {badgeColor(b)};">{b.text}</span>
    {/each}
</div>
{/if}

<style>
.corner-badges {
    position: absolute;
    top: 0.25rem;
    right: 0.25rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.15rem;
    width: max-content;
    max-width: 100%;
    z-index: 3;
    pointer-events: none;
}
.corner-badge {
    color: white;
    font-size: 0.7rem;
    font-weight: bold;
    line-height: 1;
    padding: 0.15rem 0.35rem;
    border-radius: 0.5rem;
    box-shadow: 0 0 0.25rem rgba(0, 0, 0, 0.6);
    white-space: nowrap;
}
</style>
