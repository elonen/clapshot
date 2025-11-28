<script lang="ts">
    import { flushSync } from 'svelte';


import { scale, slide } from "svelte/transition";
import Avatar from '@/lib/Avatar.svelte';
import { curUserId, curUserIsAdmin, allComments, curSubtitle, curVideo } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';
import { t } from '@/i18n';


    interface Props {
        indent?: number;
        comment: Proto3.Comment;
        ondisplaycomment?: (event: {timecode: string, drawing?: string, subtitleId?: string}) => void;
        ondeletecomment?: (event: {id: string}) => void;
        onreplytocomment?: (event: {parentId: string, commentText: string, subtitleId?: string}) => void;
        oneditcomment?: (event: {id: string, comment_text: string}) => void;
    }

    let { indent = 0, comment, ondisplaycomment, ondeletecomment, onreplytocomment, oneditcomment }: Props = $props();

let editing = $state(false);
let showActions: boolean = $state(false);

let showReply: boolean = $state(false);
let replyInput: HTMLInputElement | undefined = $state();

let commentText = $state(comment.comment);

$effect(() => {
    commentText = comment.comment;
});

function onTimecodeClick(tc: string) {
    if (ondisplaycomment) ondisplaycomment({
        timecode: tc,
        drawing: comment.drawing,
        subtitleId: comment.subtitleId
    });
}

function onClickDeleteComment() {
    var result = confirm($t('comments.deleteConfirm'));
    if (result && ondeletecomment) {
        ondeletecomment({'id': comment.id});
    }
}

function onReplySubmit() {
    if (replyInput && replyInput.value != "" && onreplytocomment)
    {
        onreplytocomment({
            parentId: comment.id,
            commentText: replyInput.value,
            subtitleId: $curSubtitle?.id
        });
        replyInput.value = "";
        showReply = false;
    }
}

function callFocus(elem: HTMLElement) {
    elem.focus();
}

function onEditFieldKeyDown(e: KeyboardEvent) {
    if ((e.key == "Enter" && !e.shiftKey) || e.key == "Escape") {
        e.preventDefault();
        e.stopPropagation();
        flushSync(() => {
            editing = false;
        });
        commentText = commentText.trim();
        if (commentText != "" && oneditcomment) {
            comment.comment = commentText;
            oneditcomment({'id': comment.id, 'comment_text': commentText});
        }
    }
}

function onEditFieldBlur() {
    if (editing) {
        editing = false;
        commentText = commentText.trim();
        comment.comment = commentText;
    }
}

function hasChildren(): boolean {
    return $allComments.filter(c => c.comment.parentId == comment.id).length > 0;
}

function getSubtitleLanguage(subtitleId: string): string {
    let sub = $curVideo?.subtitles.find(s => s.id == subtitleId);
    return sub ? sub.languageCode.toUpperCase() : "";
}

function onClickShare() {
    // Build a simple fragment link to this comment
    try {
        const base = window.location.href.split('#')[0];
        const url = `${base}#comment_${comment.id}`;
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(url).then(() => {
                alert($t('comments.linkCopied'));
            }).catch(() => { alert($t('comments.copyLink') + ": " + url); });
        } else { alert($t('comments.copyLink') + ": " + url); }
    } catch (e) {
        console.error('Failed to copy link', e);
    }
}

</script>

<div transition:scale
    id="comment_card_{comment.id}"
    class="block overflow-clip text-ellipsis rounded-lg bg-gray-800 {!!comment.timecode ? 'hover:bg-gray-700' : ''} shadow-lg shadow-black"
    style="margin-left: {indent*1.5}em"
    tabindex="0"
    role="link"
    onfocus={() => showActions=true}
    onmouseenter={() => showActions=true}
    onmouseleave={() => showActions=false}
    onclick={() => {if (comment.timecode) onTimecodeClick(comment.timecode);}}
    onkeydown={(e) => {
        if (e.key == "Escape") { editing = false; }
        else if (e.key == "Enter") { if (comment.timecode) onTimecodeClick(comment.timecode); }
    }}
>

    <div class="flex mx-2 pt-3">
        <div class="flex-none w-9 h-9 block"><Avatar username={comment.userId || comment.usernameIfnull}/></div>
        <h5 class="flex-1 ml-3 text-gray-500 self-end">{comment.usernameIfnull}</h5>
        <span class="flex-none hidden text-xs font-mono">[{comment.id}@{comment.parentId}]</span>
        <span class="pl-2 flex-0 text-xs text-right overflow-clip text-ellipsis italic whitespace-nowrap  self-end">
                <span class="text-yellow-700 hover:text-yellow-500 hover:underline cursor-pointer">
                    {comment.timecode ? comment.timecode : ""}
                </span>
                {#if comment.subtitleId}
                    <span class="text-xs text-gray-500 text-nowrap text-ellipsis">| <strong>{getSubtitleLanguage(comment.subtitleId)}</strong></span>
                {:else if comment.subtitleFilenameIfnull}
                    <br/><span class="text-xs text-gray-500 line-through" title={comment.subtitleFilenameIfnull}>{comment.subtitleFilenameIfnull}</span>
                {/if}
        </span>
    </div>

    <div class="p-2" lang="en">
        {#if editing}
            <textarea class="w-full outline-dashed bg-slate-500" rows=3 use:callFocus bind:value={commentText} onkeydown={onEditFieldKeyDown} onblur={onEditFieldBlur}></textarea>
        {:else}
            <p class="text-gray-300 text-base hyphenate">
                {comment.comment}
                {#if comment.edited}
                    <span class="text-xs italic text-gray-500"> {$t('comments.editedSuffix')}</span>
                {/if}
            </p>
        {/if}
    </div>

    {#if showActions}
    <div class="p-2 flex items-center justify-between" transition:slide="{{ duration: 200 }}">
        <!-- Left: share icon -->
        <div class="flex-none">
            <button
                class="fa fa-link border rounded-lg px-2 py-1 text-sm border-gray-500 text-gray-300 hover:bg-gray-700"
                title={$t('comments.copyLink')}
                aria-label={$t('comments.copyLink')}
                onclick={onClickShare}
            ></button>
        </div>

        <!-- Right: existing action buttons -->
        <div class="flex-1 flex justify-end">
            <button class="border rounded-lg px-1 placeholder: ml-2 text-sm border-cyan-500 text-cyan-500" onclick={()=>showReply=true}>{$t('comments.reply')}</button>
            {#if comment.userId == $curUserId || $curUserIsAdmin}
                <button class="border rounded-lg px-1 ml-2 text-sm border-cyan-600 text-cyan-600" onclick={()=>{editing=true;}}>{$t('comments.edit')}</button>
                {#if !hasChildren()}
                <button class="border rounded-lg px-1 ml-2 text-sm border-red-300 text-red-300" onclick={onClickDeleteComment}>{$t('comments.deleteShort')}</button>
                {/if}
            {/if}
        </div>
    </div>
    {/if}

    {#if showReply}
    <form class="p-2" onsubmit={(e) => {e.preventDefault(); onReplySubmit();}}>
            <input
                class="w-full border p-1 rounded bg-gray-900"
                type="text" placeholder={$t('comments.yourReply')}
                use:callFocus
                bind:this={replyInput}
        onblur={()=>showReply=false} />
        </form>
    {/if}
</div>


<style>
.hyphenate {
    -webkit-hyphens: auto;
    -moz-hyphens: auto;
    -ms-hyphens: auto;
    hyphens: auto;
    word-break: break-word;
}
button {
    cursor: pointer;
}
</style>
