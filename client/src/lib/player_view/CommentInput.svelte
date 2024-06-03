<script lang="ts">
import { createPopup } from '@picmo/popup-picker';
import { darkTheme } from 'picmo';
import '@fortawesome/fontawesome-free/css/all.min.css';
import { createEventDispatcher } from 'svelte';
import { fade } from "svelte/transition";

import { videoIsReady } from '@/stores';

const dispatch = createEventDispatcher();

let inputText: any;
let drawMode = false;
let timedComment = true;
let curColor = "red";

export function forceDrawMode(on: boolean) {
    drawMode = on;
}

function sendDrawModeToParent() {
    dispatch('button-clicked', {'action': 'draw', 'is_draw_mode': drawMode});
}
function onClickSend() {
    dispatch('button-clicked', {'action': 'send', 'comment_text': inputText, 'is_timed': timedComment});
    inputText = "";
    drawMode = false;
    sendDrawModeToParent();
}
function onClickDraw() {
    drawMode = !drawMode;
    sendDrawModeToParent();
}
function onColorSelected(c: string) {
    curColor = c;
    dispatch('button-clicked', {'action': 'color_select', 'color': c});
}
function onUndoRedo(is_undo: boolean) {
    if (is_undo) {
        dispatch('button-clicked', {'action': 'undo'});
    } else {
        dispatch('button-clicked', {'action': 'redo'});
    }
}

function onTextChange(e: any) {
    if (e.target.value.length > 0) {
        dispatch('button-clicked', {'action': 'text_input'});
    }
    return false;
}

// Picmo emoji picker
let emojiPicker: any = null;
function onEmojiPicker(e: any)
{
    if (!emojiPicker) {
            emojiPicker = createPopup({
                theme: darkTheme }, {
            referenceElement: e.target,
            triggerElement: e.target,
            position: 'right-end',
            className: 'my-picmo-popup',
        });
        emojiPicker.addEventListener('emoji:select', (selection: any) => {
            inputText = (inputText ? inputText : '') + selection.emoji;
        });
    }
    emojiPicker.toggle();
}

</script>


<div class="relative">
    <div id="pickerContainer"></div> <!-- Picmo emoji picker -->

    <!-- Color selector -->
    {#if drawMode}
        <div class="absolute w-full top-[-3em] bg-gray-900 h-10 rounded-md flex place-content-center" transition:fade="{{duration: 100}}">
            <button type="button" class="fas fa-undo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 mx-2 rounded-lg" title="Undo" on:click={()=>onUndoRedo(true)}/>
            <button type="button" class="fas fa-redo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 mx-2 rounded-lg" title="Redo" on:click={()=>onUndoRedo(false)}/>

            {#each ["red", "green", "blue", "cyan", "yellow", "black", "white"] as c}
                <button type="button" class="{(curColor==c) ? 'border-2 border-gray-100' : 'border border-gray-600'}  inline-block w-6 h-6 m-2 rounded-lg" style="background: {c};" on:click="{() => onColorSelected(c)}"/>
            {/each}
        </div>
    {/if}

    <form on:submit|preventDefault={onClickSend} class="flex justify-left rounded-lg shadow-lg bg-gray-800 text-left p-2 w-full" >

        <input
            bind:value={inputText}
            on:input={onTextChange}
            class="flex-1 p-2 bg-gray-700 rounded-lg" placeholder="Add a comment{timedComment ? ' - at current time' :''}..." />

        <button type="button"
            class="far fa-smile text-gray-400 hover:text-yellow-500 w-8 h-8 text-[1.2em]"
            on:click={onEmojiPicker} />

        {#if $videoIsReady}
            <button type="button"
                title="Comment is time specific?"
                class="scale-90 {timedComment ? 'text-amber-600' : 'text-gray-500'}"
                on:click="{ () => timedComment = !timedComment }">
                <span class="fa-stack">
                    <i class="fa-solid fa-stopwatch fa-stack-2x"></i>
                    {#if !timedComment}
                        <i class="fa-solid fa-x fa-stack-2x text-red-800"></i>
                    {/if}
                </span>
            </button>

            <button type="button"
                on:click={onClickDraw}
                class="{drawMode ? 'border-2' : ''} fas fa-pen-fancy inline-block h-9 px-3 py-2.5 ml-2 bg-cyan-700 text-white rounded-lg shadow-md hover:bg-cyan-500 hover:shadow-lg focus:bg-cyan-700 focus:shadow-lg focus:outline-none focus:ring-0 active:shadow-lg transition duration-150 ease-in-out"
                title="Draw on video">
            </button>
        {/if}

        <button type="submit"
            disabled={!inputText}
            class="inline-block h-9 px-4 py-2 ml-2 text-sm bg-blue-700 text-white rounded-lg shadow-md hover:bg-blue-500 hover:shadow-lg focus:bg-blue-700 focus:shadow-lg focus:outline-none focus:ring-0 active:bg-blue-800 active:shadow-lg transition duration-150 ease-in-out">
            Send
        </button>

    </form>
</div>


<style>

button {
    transition: 0.1s ease-in-out;
}
button:disabled {
    opacity: 0.5;
    background-color: gray;
}
</style>

