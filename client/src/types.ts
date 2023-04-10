import * as Proto3 from '../../protobuf/libs/typescript';

export class IndentedComment {
    comment!: Proto3.Comment;
    indent!: number;
}

export interface UserMenuItem {
    type: string;
    label: string;
    data: string;
}
