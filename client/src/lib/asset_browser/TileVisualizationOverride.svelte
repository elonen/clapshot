<script lang="ts">
import * as Proto3 from '@clapshot_protobuf/typescript';
import { rgbToCssColor } from './utils';

export let vis: Proto3.PageItem_FolderListing_Item_Visualization;
export let extra_styles: string = "";

</script>

<div class="w-full h-full overflow-clip inline-block relative rounded-md" role="img" style="{extra_styles}">
    <div class="w-full h-full flex items-center justify-center">
        {#if vis.icon?.faClass}
            <i
                class="{vis.icon.faClass.classes}"
                style="color: {vis.icon.faClass.color ? rgbToCssColor(vis.icon.faClass.color.r, vis.icon.faClass.color.g, vis.icon.faClass.color.b) : '#fff'}; font-size: {vis.icon.size || 2.0}em;"
            ></i>
        {:else if vis.icon?.imgUrl}
            <img src={vis.icon.imgUrl} class="w-1/2 h-1/2" alt="icon img"/>
        {/if}
    </div>
</div>