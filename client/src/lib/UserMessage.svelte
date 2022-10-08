<script lang="ts">
import { slide, fade } from "svelte/transition";

export let msg: any = null;

let show_details: boolean = false;
</script>

<div class="border-t border-slate-800 p-1 m-1 mx-6 inline-block w-[90%]">
    <span class="font-mono text-sm pr-1 {(msg.event_name == "error") ? 'text-red-400': 'text-green-400'}">
        {msg.event_name.toUpperCase()}
    </span>
    <span class="text-xs text-gray-500 pl-2 border-l border-gray-400">{msg.created}</span>

    {#if msg.event_name == "error"}
        <span class="font-mono text-xs pl-2 border-l border-gray-400 line-through text-gray-700">
            {msg.ref_video_hash}
        </span>
    {:else}
        <a class="font-mono text-xs pl-2 border-l border-gray-400 text-amber-600"
            href="/?vid={msg.ref_video_hash}">
            {msg.ref_video_hash}
        </a>
    {/if}

    <span class="text-gray-400 text-sm pl-2 border-l border-gray-400 pr-2">{msg.message}</span>

    {#if msg.details }    
        <span class="text-xs text-gray-500 pl-2 border-l border-gray-400"></span>
        {#if show_details}
            <i class="fa fa-chevron-up text-[#cca] cursor-pointer" on:click={()=>{show_details=false}}></i>
            <div
                class="bg-[#cca] font-mono rounded-md mt-2 p-2 text-black text-xs block"
                transition:slide="{{ duration: 200 }}">
                {msg.details}
            </div>
        {:else}
            <i class="fa fa-chevron-down text-[#cca] cursor-pointer" on:click={()=>{show_details=true}}></i>
        {/if}
    {/if}
</div>

<style>
    @import '@fortawesome/fontawesome-free/css/all.min.css';
</style>
