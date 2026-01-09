export type ExportOptionType = 'number' | 'boolean' | 'string' | 'select';

export interface ExportOption {
    id: string;
    label: string;
    type: ExportOptionType;
    default: number | boolean | string;
    choices?: { value: string; label: string }[];
}

export interface ExportedComment {
    timecode: string;        // "HH:MM:SS.mmm" or empty string for untimed
    text: string;            // merged comment text (parent + replies)
    hasDrawing: boolean;
    username?: string;
    created?: Date;
}

export interface ExportContext {
    comments: ExportedComment[];
    title: string;
    fps: number;
    duration?: number;       // total duration in seconds
}

export interface CommentExporter {
    id: string;
    name: string;
    extension: string;
    options: ExportOption[];
    export(ctx: ExportContext, opts: Record<string, unknown>): string;
}
