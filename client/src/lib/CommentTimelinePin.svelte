<script lang="ts">
    import {createEventDispatcher} from 'svelte';
    const dispatch = createEventDispatcher();

    import Avatar from './Avatar.svelte';
    import {hexColorForUsername} from './Avatar.svelte';

    export let id: string = ""; // The id of the comment
    export let username: string = "";
    export let comment: string = "";
    export let avatar_url: string = "";
    export let x_loc: string = "";

</script>

<div class="pin" style="left: {x_loc*100}%">
    <div class="line shadow-sm shadow-gray-600" style="background-color: {hexColorForUsername(username)}"></div>
    <div class="sphere shadow-sm shadow-gray-800" style="background-color: {hexColorForUsername(username)}"
        title="{username}: {comment}"
        on:click={() => dispatch('click', {id})}></div>
</div>
<style>
    .pin {
        z-index: 2000;
        position: relative;
        width: 0;
        height: 0;
    }

    .line {
        position: absolute;
        top: 0;
        width: 3px;
        height: 2.25em;
        transform: translate(-50%, -100%);
        pointer-events: none;
    }

    .sphere {
        position: absolute;
        bottom: 0;
        width: 8px;
        height: 8px;
        border-radius: 50%;
        transform: translate(-50%, 0%);
        cursor: pointer;
        transition: width 0.1s, height 0.1s, transform 0.1s;
    }

    .sphere:hover {
        width: 16px;
        height: 16px;
        transform: translate(-50%, 20%);
    }
</style>