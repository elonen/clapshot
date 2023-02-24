<script lang="ts">
    export let onRename: Function = null;
    export let onDel: Function = null;

    let pos = { x: 0, y: 0 }
    let menu = { w: 0, h: 0 }
    let browser = { w:0, h: 0 }
    let showMenu = false;

    function toggleVisibility(e) {
        showMenu = true
        if (showMenu) {
            browser = { w: window.innerWidth, h: window.innerHeight };
            pos = { x: e.clientX, y: e.clientY };
            if (browser.h -  pos.y < menu.h)
                pos.y = pos.y - menu.h
            if (browser.w -  pos.x < menu.w)
                pos.x = pos.x - menu.w
        }
    }
    function getContextMenuDimension(node) {
        let height = node.offsetHeight
        let width = node.offsetWidth
        menu = { h: height, w: width }
    }

    function onButtonClick(e) {
        toggleVisibility(e)
    }
    function onKeyPress(e) {
        if (e.key === 'Enter') {
            toggleVisibility(e)
        }
    }
    function onPageClick(e) {
        showMenu = false;
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
                onDel();
                showMenu = false;
            },
            'displayText': "Delete",
            'class': 'fa-solid fa-trash-can'
        },
    ]
</script>


{#if showMenu}
<nav use:getContextMenuDimension style="position: absolute; top:{pos.y}px; left:{pos.x}px">
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


<span on:click|stopPropagation={onButtonClick} on:keypress|stopPropagation={onKeyPress} class="text-gray-400 hover:text-white right:0">...</span>
<svelte:window on:click={onPageClick} />


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
