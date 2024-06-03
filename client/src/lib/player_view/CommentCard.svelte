<script lang="ts">

import { createEventDispatcher } from 'svelte';
import { scale, slide } from "svelte/transition";
import Avatar from '@/lib/Avatar.svelte';
import { curUserId, curUserIsAdmin, allComments } from '@/stores';
import * as Proto3 from '@clapshot_protobuf/typescript';

const dispatch = createEventDispatcher();

export let indent: number = 0;
export let comment: Proto3.Comment;

let editing = false;
let showActions: boolean = false;

let showReply: boolean = false;
let replyInput: HTMLInputElement;

function onTimecodeClick(tc: string) {
    dispatch("display-comment", {'timecode': tc, 'drawing': comment.drawing});
}

function onClickDeleteComment() {
    var result = confirm("Delete comment?");
    if (result) {
        dispatch("delete-comment", {'id': comment.id});
    }
}

function onReplySubmit() {
    if (replyInput.value != "")
    {
        dispatch("reply-to-comment", {'parent_id': comment.id, 'comment_text': replyInput.value});
        replyInput.value = "";
        showReply = false;
    }
}

function callFocus(elem: HTMLElement) {
    elem.focus();
}

function onEditFieldKeyUp(e: KeyboardEvent) {
    if ((e.key == "Enter" && !e.shiftKey) || e.key == "Escape") {
        console.log("Enter pressed");
        editing = false;
        comment.comment = comment.comment.trim();
        if (comment.comment != "")
            dispatch("edit-comment", {'id': comment.id, 'comment_text': comment.comment});
    }
}

function hasChildren(): boolean {
    return $allComments.filter(c => c.comment.parentId == comment.id).length > 0;
}

</script>

<div transition:scale
    id="comment_card_{comment.id}"
    class="block overflow-clip rounded-lg bg-gray-800 {!!comment.timecode ? 'hover:bg-gray-700' : ''} shadow-lg shadow-black"
    style="margin-left: {indent*1.5}em"
    tabindex="0"
    role="link"
    on:focus="{() => showActions=true}"
    on:mouseenter="{() => showActions=true}"
    on:mouseleave="{() => showActions=false}"
    on:click = "{() => {if (comment.timecode) onTimecodeClick(comment.timecode);}}"
    on:keydown={(e) => {
        if (e.key == "Escape") { editing = false; }
        else if (e.key == "Enter") { if (comment.timecode) onTimecodeClick(comment.timecode); }
    }}
>

    <div class="flex mx-2 pt-3">
        <div class="flex-none w-9 h-9 block"><Avatar username="{comment.userId || comment.usernameIfnull}"/></div>
        <h5 class="flex-1 ml-3 text-gray-500 self-end">{comment.usernameIfnull}</h5>
        <span class="flex-none hidden text-xs font-mono">[{comment.id}@{comment.parentId}]</span>
        <span
            class="pl-2 flex-0 text-xs italic whitespace-nowrap text-yellow-700 hover:text-yellow-500 hover:underline cursor-pointer self-end">
                {comment.timecode ? comment.timecode : ""}
        </span>
    </div>

    <div class="p-2" lang="en">
        {#if editing}
            <textarea class="w-full outline-dashed bg-slate-500" rows=3 use:callFocus bind:value={comment.comment} on:keyup={onEditFieldKeyUp} on:blur="{()=>{editing=false; comment.comment = comment.comment.trim()}}"></textarea>
        {:else}
            <p class="text-gray-300 text-base hyphenate">
                {comment.comment}
                {#if comment.edited}
                    <span class="text-xs italic text-gray-500"> (edited)</span>
                {/if}
            </p>
        {/if}
    </div>

    {#if showActions}
    <div class="p-2 flex place-content-end" transition:slide="{{ duration: 200 }}">
        <button class="border rounded-lg px-1 placeholder: ml-2 text-sm border-cyan-500 text-cyan-500" on:click={()=>showReply=true}>Reply</button>
        {#if comment.userId == $curUserId || $curUserIsAdmin}
            <button class="border rounded-lg px-1 ml-2 text-sm border-cyan-600 text-cyan-600" on:click="{()=>{editing=true;}}">Edit</button>
            {#if !hasChildren()}
            <button class="border rounded-lg px-1 ml-2 text-sm border-red-300 text-red-300" on:click={onClickDeleteComment}>Del</button>
            {/if}
        {/if}
    </div>
    {/if}

    {#if showReply}
        <form class="p-2" on:submit|preventDefault={onReplySubmit}>
            <input
                class="w-full border p-1 rounded bg-gray-900"
                type="text" placeholder="Your reply..."
                use:callFocus
                bind:this={replyInput}
                on:blur="{()=>showReply=false}" />
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
</style>