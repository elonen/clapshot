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

export type StringMap = { [key: string]: string };
