<script lang="ts">
    let input_element: HTMLElement;
    const placeholder_txt: string = "Add a comment...";

    let count: number = 0;
    function onClickSend() {
        input_element.textContent = placeholder_txt;
        count += 1;
    }

    function hidePlaceholder() {
        if (input_element.textContent.includes(placeholder_txt)) {
            input_element.textContent = "";
        }
    }
</script>


<div class="flex justify-left block rounded-lg shadow-lg shadow-lg bg-gray-800 text-left p-4 w-full" >
    <img src="https://mdbootstrap.com/img/new/avatars/2.jpg" class="rounded-full p-2 w-12 h-12" alt="" loading="lazy" />

    <span bind:this={input_element} class="flex-1 p-2 bg-gray-700 rounded-lg" contenteditable on:focus="{hidePlaceholder}" id="custom_textinput">
        <span class="text-gray-500">{placeholder_txt} ({count} clicks)...</span>
    </span>

    <button type="button"
        on:click={onClickSend}
        class="inline-block h-12 px-6 py-2.5 ml-2 bg-blue-700 text-white rounded-lg shadow-md hover:bg-blue-700 hover:shadow-lg focus:bg-blue-700 focus:shadow-lg focus:outline-none focus:ring-0 active:bg-blue-800 active:shadow-lg transition duration-150 ease-in-out">
        Send
    </button>
</div>

<style>
</style>
    
    