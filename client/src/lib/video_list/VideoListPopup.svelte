<script lang="ts">
    import { onDestroy, onMount } from 'svelte';

    export let x = 0;
    export let y = 0;
    export let onRename: Function = null;
    export let onDelete: Function = null;

    let menu_el = null;
    let removed = false;

    // Make sure the menu doesn't go off the screen
	$: (() => {
		if (!menu_el) return;
		const rect = menu_el.getBoundingClientRect();
		x = Math.min(window.innerWidth - rect.width, x);
		if (y > window.innerHeight - rect.height) y -= rect.height;
    })();

    export function hide() {
        removed = true;
    }
    
    let menuItems = [
        {
            'name': 'rename',
            'displayText': "Rename",
            'class': 'fa-solid fa-edit',
            'handler': () => {
                // Delay onRename() to next cycle to allow the menu to close first
                setTimeout(() => { onRename(); }, 0);
                hide();
            },
        },
        {
            'name': 'trash',
            'displayText': "Delete",
            'class': 'fa-solid fa-trash-can',
            'handler': () => {
                setTimeout(() => { onDelete(); }, 0);
                hide();
            },
        },
    ];

    onMount(() => {
        menu_el.hide = hide;    // Export hide() to the DOM element
    });
    
</script>

{#if !removed}
<nav style="position: absolute; z-index: 30; top:{y}px; left:{x}px"
    bind:this={menu_el}
>
    <div class="popupmenu">
        <ul>
            {#each menuItems as item}
                {#if item.name == "hr"}
                    <hr>
                {:else}
                    <li><button on:click|stopPropagation={item.handler}><i class={item.class}></i>{item.displayText}</button></li>
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
