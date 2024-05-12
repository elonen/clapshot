<script lang="ts">
import { createEventDispatcher, onMount } from 'svelte';
import * as Proto3 from '@clapshot_protobuf/typescript';

export let x = 0;
export let y = 0;
export let menuLines: Proto3.ActionDef[] = [];

let menu_el: HTMLElement | null = null;
let removed = false;
const dispatch = createEventDispatcher();

function moveKeepMenuOnScreen() {
    if (!menu_el) return;
    const rect = menu_el.getBoundingClientRect();
        x = Math.min(window.innerWidth - rect.width, x);
        if (y > window.innerHeight - rect.height) y -= rect.height;
}
$: moveKeepMenuOnScreen();  // $: = called on any dependency change

export function hide() {
    removed = true;
    dispatch("hide");
}

function onClickItem(item: Proto3.ActionDef) {
    dispatch("action", {action: item });
    hide();
}

onMount(() => {
    if (!menu_el) return;
    // @ts-ignore
    menu_el.hide = hide;    // Export hide() to the DOM element
});

function fmtColorToCSS(c: Proto3.Color | null | undefined) {
    if (!c) return "black";
    return `rgb(${c.r},${c.g},${c.b})`;
}
</script>


{#if !removed}
<nav style="position: absolute; z-index: 30; top:{y}px; left:{x}px"
    bind:this={menu_el}
>
    <div class="popupmenu">
        <ul>
            {#each menuLines as it}
                {#if it.uiProps?.label?.toLowerCase() == "hr" && !it.action?.code}
                    <hr>
                {:else if it.uiProps}
                    <li><button on:click|stopPropagation={()=>{onClickItem(it)}}>
                        {#if it.uiProps.icon?.faClass}<i class={it.uiProps.icon?.faClass.classes} style="color: {fmtColorToCSS(it.uiProps.icon?.faClass.color)}"></i>{/if}
                        {#if it.uiProps.icon?.imgUrl}<img alt="" src={it.uiProps.icon?.imgUrl} style="max-width: 2em; max-height: 2em;"/>{/if}
                        {it.uiProps.label}
                    </button></li>
                {/if}
            {/each}
        </ul>
    </div>
</nav>
{/if}

<svelte:window on:click={hide} />


<style>
@import '@fortawesome/fontawesome-free/css/all.min.css';
* {
    padding: 0;
    margin: 0;
}
.popupmenu{
    display: inline-flex;
    border: 1px #999 solid;
    width: 170px;
    background-color: #fff;
    border-radius: 10px;
    overflow: hidden;
    flex-direction: column;
    box-shadow: 0px 0px 8px 0px rgba(0,0,0,0.75);
}
.popupmenu ul{
    margin: 6px;
}
ul li{
    display: block;
    list-style-type: none;
    width: 1fr;
}
ul li button{
    font-size: 1rem;
    color: #222;
    width: 100%;
    height: 30px;
    text-align: left;
    border: 0px;
    background-color: #fff;
}
ul li button:hover{
    color: #000;
    text-align: left;
    border-radius: 5px;
    background-color: #ddd;
}
ul li button i{
    padding: 0px 15px 0px 10px;
}
ul li button i.fa-square{
    color: #fff;
}
ul li button:hover > i.fa-square{
    color: #eee;
}
ul li button:hover > i.warning{
    color: crimson;
}
:global(ul li button.info:hover){
    color: navy;
}
hr{
    border: none;
    border-bottom: 1px solid #ccc;
    margin: 5px 0px;
}
</style>
