<script lang="ts">
import { slide, fade } from "svelte/transition";

export let msg: any = null;

let show_details: boolean = false;
</script>

<div class="border-t border-slate-800 p-1 m-1 mx-6 inline-block w-[90%]"
    on:mouseenter="{() => show_details=true}"
    on:mouseleave="{() => show_details=false}"
>
    <span class="font-mono text-sm pr-1 {(msg.event_name == "error") ? 'text-red-400': 'text-green-400'}">
        {msg.event_name.toUpperCase()}
    </span>
    <span class="text-xs text-gray-500 pl-2 border-l border-gray-400">{msg.created}</span>
    <span class="text-gray-400 text-sm pr-2">{msg.message}</span>

    {#if msg.event_name == "error"}
        <span class="font-mono text-xs pl-2 border-l border-gray-400 line-through text-gray-700">
            {msg.ref_video_hash}
        </span>
    {:else}
        <a class="font-mono text-xs pl-2 border-l border-gray-400 text-amber-800'"
            href="/?vid={msg.ref_video_hash}">
            {msg.ref_video_hash}
        </a>
    {/if}

    {#if msg.details}
        {#if show_details}
            <div
                class="bg-[#cca] font-mono rounded-md mt-2 p-2 text-black text-xs block"
                transition:slide="{{ duration: 200 }}">
                {msg.details}
            </div>
        {:else}
            <span
                class="text-xs text-gray-500 pl-2 border-l border-gray-400"
                transition:fade="{{ duration: 200 }}">
                details...
            </span>
        {/if}
    {/if}
</div>
