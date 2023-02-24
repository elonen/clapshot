<script lang="ts">
    import { all_popup_hide_funcs } from '../../stores.js';

    export let onRename: Function = null;
    export let onDelete: Function = null;

    let pos = { x: 0, y: 0 }
    let showMenu = false;

    export function show(e) {
        $all_popup_hide_funcs.forEach((func) => { func(); });
        showMenu = true;
        pos = { x: e.clientX, y: e.clientY };
        pos.y -= 16; // Offset a bit to make it look better
    }

    export function hide() {
        showMenu = false
    }
    $all_popup_hide_funcs.push(hide);
    
    function onButtonClick(e) {
        show(e)
    }
    function onKeyPress(e) {
        if (e.key === 'Enter') {
            show(e)
        }
    }

    let menuItems = [
        {
            'name': 'rename',
            'handler': () => {
                showMenu = false;
                // Delay onRename() to next cycle to allow the menu to close first
                setTimeout(() => { onRename(); }, 0);
            },
            'displayText': "Rename",
            'class': 'fa-solid fa-edit'
        },
        {
            'name': 'trash',
            'handler': () => {
                onDelete();
                showMenu = false;
            },
            'displayText': "Delete",
            'class': 'fa-solid fa-trash-can'
        },
    ];

</script>


{#if showMenu}
<nav class="any-popup-menu" style="position: absolute; z-index: 30; top:{pos.y}px; left:{pos.x}px">
    <div class="navbar" id="navbar">
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
    .navbar{
        display: inline-flex;
        border: 1px #999 solid;
        width: 170px;
        background-color: #fff;
        border-radius: 10px;
        overflow: hidden;
        flex-direction: column;
    }
    .navbar ul{
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
        background-color: #eee;
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
