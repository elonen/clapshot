import * as Proto3 from '@clapshot_protobuf/typescript';

export class VideoListDefItem {
    id!: string;
    obj!: Proto3.PageItem_FolderListing_Item;
}

// Convert UI folder items to a protobuf FolderItemID array
export function folderItemsToIDs(items: Proto3.PageItem_FolderListing_Item[]): Proto3.FolderItemID[] {
    function conv(it: Proto3.PageItem_FolderListing_Item): Proto3.FolderItemID {
        if (it && it.video) { return {videoId: it.video.id}; }
        else if (it && it.folder) { return {folderId: it.folder.id}; }
        else { alert("UI BUG: unknown item type: " + JSON.stringify(it)); return {videoId: ""}; }
    }
    return items.map(conv);
}
