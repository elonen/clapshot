<script lang="ts">
import { slide } from "svelte/transition";
import * as Proto3 from '@clapshot_protobuf/typescript';

export let msg: Proto3.UserMessage;
let showDetails: boolean = false;

function isError(msg: Proto3.UserMessage): boolean {
    return msg.type == Proto3.UserMessage_Type.ERROR;
}

function msgTypeName(msg: Proto3.UserMessage): string {
    switch (msg.type) {
        case Proto3.UserMessage_Type.OK:
            return 'OK';
        case Proto3.UserMessage_Type.ERROR:
            return 'ERROR';
        case Proto3.UserMessage_Type.PROGRESS:
            return 'PROGRESS';
        default:
            return '';
    }
}

function dateObjToISO(d: Date|undefined): string {
    if (d == null) return '';
    var tzo = -d.getTimezoneOffset(),
        dif = tzo >= 0 ? '+' : '-',
        pad = function(num: number) { return (num < 10 ? '0' : '') + num; };

    return d.getFullYear() +
        '-' + pad(d.getMonth() + 1) +
        '-' + pad(d.getDate()) +
        ' ' + pad(d.getHours()) +
        ':' + pad(d.getMinutes()) +
        ':' + pad(d.getSeconds()) +
        ' ' + dif + pad(Math.floor(Math.abs(tzo) / 60)) +
        ':' + pad(Math.abs(tzo) % 60);
}
</script>

<div class="border-t border-slate-800 p-1 m-1 mx-6 inline-block w-[90%]">
    <span class="font-mono text-sm pr-1 {isError(msg) ? 'text-red-400': 'text-green-400'}">
        {msgTypeName(msg)}
    </span>
    <span class="text-xs text-gray-500 pl-2 border-l border-gray-400">{dateObjToISO(msg.created)}</span>

    {#if msg.refs?.videoId }
        {#if isError(msg)}
            <span class="font-mono text-xs pl-2 border-l border-gray-400 line-through text-gray-700">
                {msg.refs.videoId}
            </span>
        {:else}
            <a class="font-mono text-xs pl-2 border-l border-gray-400 text-amber-600"
                href="/?vid={msg.refs.videoId}">
                {msg.refs.videoId}
            </a>
        {/if}
    {/if}

    <span class="text-gray-400 text-sm pl-2 border-l border-gray-400 pr-2">{msg.message}</span>

    {#if msg.details }
        <span class="text-xs text-gray-500 pl-2 border-l border-gray-400"></span>
        {#if showDetails}
            <i class="fa fa-chevron-down text-[#cca] cursor-pointer"
                tabindex="0"
                role="link"
                on:keyup={e=> {if (e.key==='Enter') showDetails=false; }}
                on:click={()=>{showDetails=false}}></i>
            <div
                class="bg-[#cca] font-mono rounded-md mt-2 p-2 text-black text-xs block"
                transition:slide="{{ duration: 200 }}">
                {msg.details}
            </div>
        {:else}
            <i class="fa fa-chevron-right text-[#cca] cursor-pointer"
                tabindex="0"
                role="link"
                on:keyup={e=> {if (e.key==='Enter') showDetails=true; }}
                on:click={()=>{showDetails=true}}></i>
        {/if}
    {/if}
</div>

<style>
@import '@fortawesome/fontawesome-free/css/all.min.css';
</style>
