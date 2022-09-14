<script lang="ts">
    import { createEventDispatcher } from 'svelte';
    import { fade, blur, fly, slide, scale } from "svelte/transition";
    import { video_is_ready, cur_user_pic } from '../stores.js';

    const dispatch = createEventDispatcher();

    let input_text: any;
    let draw_mode = false;
    let timed_comment = true;
    let cur_color = "red";

    export function forceDrawMode(on: boolean) {
        draw_mode = on;
    }

    function sendDrawModeToParent() {
        dispatch('button-clicked', {'action': 'draw', 'is_draw_mode': draw_mode});
    }

    function onClickSend() {
        dispatch('button-clicked', {'action': 'send', 'comment_text': input_text, 'is_timed': timed_comment});
        input_text = "";
        draw_mode = false;
        sendDrawModeToParent();
    }
    function onClickDraw() {
        draw_mode = !draw_mode;
        sendDrawModeToParent();
    }
    function onColorSelected(c: string) {
        cur_color = c;
        dispatch('button-clicked', {'action': 'color_select', 'color': c});
    }
    function onUndoRedo(is_undo: boolean) {
        if (is_undo) {
            dispatch('button-clicked', {'action': 'undo'});
        } else {
            dispatch('button-clicked', {'action': 'redo'});
        }
    }

</script>


<form on:submit|preventDefault={onClickSend} class="flex justify-left block rounded-lg shadow-lg shadow-lg bg-gray-800 text-left p-4 w-full" >

    <img src={$cur_user_pic} class="rounded-full p-2 w-12 h-12" alt="" loading="lazy" />

    <input 
        bind:value={input_text} 
        class="flex-1 p-2 bg-gray-700 rounded-lg" placeholder="Add a comment..." />

    {#if $video_is_ready}
        <button type="button"
            title="Comment is time specific?"
            class="fas fa-stopwatch w-12 h-12 text-[1.75em] {timed_comment ? 'text-blue-400' : 'text-gray-500'}"
            on:click="{ () => timed_comment = !timed_comment }" />

        <button type="button"
            on:click={onClickDraw}
            class="{draw_mode ? 'border-2' : ''} fas fa-pen-fancy inline-block h-12 px-6 py-2.5 ml-2 bg-cyan-700 text-white rounded-lg shadow-md hover:bg-cyan-500 hover:shadow-lg focus:bg-cyan-700 focus:shadow-lg focus:outline-none focus:ring-0 active:shadow-lg transition duration-150 ease-in-out"
            title="Draw on video">
        </button>
    {/if}

    <button type="submit"
        disabled={!input_text}
        class="inline-block h-12 px-6 py-2.5 ml-2 bg-blue-700 text-white rounded-lg shadow-md hover:bg-blue-500 hover:shadow-lg focus:bg-blue-700 focus:shadow-lg focus:outline-none focus:ring-0 active:bg-blue-800 active:shadow-lg transition duration-150 ease-in-out">
        Send
    </button>

</form>

<!-- Color selector -->
{#if draw_mode}
    <div class="bg-gray-900 rounded-lg flex place-content-end" transition:slide>
        <button type="button" class="fas fa-undo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 m-2 rounded-lg" title="Undo" on:click={()=>onUndoRedo(true)}/>
        <button type="button" class="fas fa-redo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 m-2 rounded-lg" title="Redo" on:click={()=>onUndoRedo(false)}/>

        {#each ["red", "green", "blue", "cyan", "yellow", "black", "white"] as c}
            <button type="button" class="{(cur_color==c) ? 'border-4' : 'border'} border-gray-100 inline-block w-10 h-10 m-2 rounded-full" style="background: {c};" on:click="{() => onColorSelected(c)}"/>
        {/each}
    </div>
{/if}

<style>
    @import '@fortawesome/fontawesome-free/css/all.min.css';

    button {
        transition: 0.1s ease-in-out;
    }
    button:disabled {
        opacity: 0.5;
        background-color: gray;
    }
</style>

    