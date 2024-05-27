import * as Proto3 from '@clapshot_protobuf/typescript';

export class IndentedComment {
    comment!: Proto3.Comment;
    indent!: number;
}

export interface UserMenuItem {
    type: string;
    label: string;
    data: string;
}

export interface MediaProgressReport {
    mediaFileId?: string;
    msg?: string;
    progress?: number;      // 0-1
    received_ts: number;    // timestamp, for expiring old reports
}

export type StringMap = { [key: string]: string };
