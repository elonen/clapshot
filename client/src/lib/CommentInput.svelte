<script lang="ts">
    import { createPopup } from '@picmo/popup-picker';
    import { darkTheme } from 'picmo';
    import { TwemojiRenderer  } from '@picmo/renderer-twemoji';
    
    import { createEventDispatcher } from 'svelte';
    import { fade } from "svelte/transition";
    import { video_is_ready } from '../stores.js';

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

  // Picmo emoji picker
  let emoji_picker: any = null;
  function onEmojiPicker(e: any) 
  {
    if (!emoji_picker) {
            emoji_picker = createPopup({
                theme: darkTheme,                
                renderer: new TwemojiRenderer()}, {
            referenceElement: e.target,
            triggerElement: e.target,            
            position: 'right-end',
            className: 'my-picmo-popup',
        });
        emoji_picker.addEventListener('emoji:select', (selection: any) => {
            input_text = (input_text ? input_text : '') + selection.emoji;
        });
    }
    emoji_picker.toggle();
}

</script>


<div class="relative">
<div id="pickerContainer"></div> <!-- Picmo emoji picker -->

<!-- Color selector -->
{#if draw_mode}
    <div class="absolute w-full top-[-3em] bg-gray-900 h-10 rounded-md flex place-content-center" transition:fade="{{duration: 100}}">
        <button type="button" class="fas fa-undo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 mx-2 rounded-lg" title="Undo" on:click={()=>onUndoRedo(true)}/>
        <button type="button" class="fas fa-redo text-gray-500 hover:text-gray-100 active:text-gray-400 inline-block w-10 h-10 mx-2 rounded-lg" title="Redo" on:click={()=>onUndoRedo(false)}/>

        {#each ["red", "green", "blue", "cyan", "yellow", "black", "white"] as c}
            <button type="button" class="{(cur_color==c) ? 'border-2 border-gray-100' : 'border border-gray-600'}  inline-block w-6 h-6 m-2 rounded-lg" style="background: {c};" on:click="{() => onColorSelected(c)}"/>
        {/each}
    </div>
{/if}

<form on:submit|preventDefault={onClickSend} class="flex justify-left rounded-lg shadow-lg bg-gray-800 text-left p-2 w-full" >

    <input 
        bind:value={input_text} 
        class="flex-1 p-2 bg-gray-700 rounded-lg" placeholder="Add a comment{timed_comment ? ' - at current time' :''}..." />

    <button type="button"
        class="far fa-smile text-gray-400 hover:text-yellow-500 w-8 h-8 text-[1.2em]"
        on:click={onEmojiPicker} />

    {#if $video_is_ready}
        <button type="button"
            title="Comment is time specific?"
            class="scale-90 {timed_comment ? 'text-amber-600' : 'text-gray-500'}"
            on:click="{ () => timed_comment = !timed_comment }">
            <span class="fa-stack">
                <i class="fa-solid fa-stopwatch fa-stack-2x"></i>
                {#if !timed_comment}
                    <i class="fa-solid fa-x fa-stack-2x text-red-800"></i>
                {/if}            
            </span>
        </button>

        <button type="button"
            on:click={onClickDraw}
            class="{draw_mode ? 'border-2' : ''} fas fa-pen-fancy inline-block h-9 px-3 py-2.5 ml-2 bg-cyan-700 text-white rounded-lg shadow-md hover:bg-cyan-500 hover:shadow-lg focus:bg-cyan-700 focus:shadow-lg focus:outline-none focus:ring-0 active:shadow-lg transition duration-150 ease-in-out"
            title="Draw on video">
        </button>
    {/if}

    <button type="submit"
        disabled={!input_text}
        class="inline-block h-9 px-4 py-2 ml-2 text-sm bg-blue-700 text-white rounded-lg shadow-md hover:bg-blue-500 hover:shadow-lg focus:bg-blue-700 focus:shadow-lg focus:outline-none focus:ring-0 active:bg-blue-800 active:shadow-lg transition duration-150 ease-in-out">
        Send
    </button>

</form>
</div>

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

    