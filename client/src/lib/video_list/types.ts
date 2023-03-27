import type * as Proto3 from '../../../../protobuf/libs/typescript';

export interface ClapshotCommentJson
{
    id: number;
    video_hash: string;
    parent_id: number | null;

    created: number;  // unix timestamp
    edited: number | null;

    user_id: string;
    username: string;
    comment: string;
    timecode: string | null;
    drawing: string | null;
}

// -----

export class VideoListPopupMenuItem {
    label: string;
    icon_class: string|null;
    key_shortcut: string|null;
    action: string;
}

// -----

export class VideoListDefItem {
    id: string;
    obj: Proto3.PageItem_FolderListing_Item;
}
