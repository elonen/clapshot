<script lang="ts">

    export let post_url: string;

    let input_el: HTMLInputElement;
    let progress_bar: HTMLProgressElement;
    let status_txt: string = "";
    let uploading_now: boolean = false;
    let form: HTMLFormElement

    function upload()
    {
        var file = input_el.files[0];
        var formdata = new FormData();
        formdata.append("fileupload", file);
        // store filename in headers
        var ajax = new XMLHttpRequest();
        ajax.upload.addEventListener("progress", progressHandler, false);
        ajax.addEventListener("load", completeHandler, false);
        ajax.addEventListener("error", errorHandler, false) ;
        ajax.addEventListener("abort", abortHandler, false);
        ajax.open("POST", post_url); 
        ajax.setRequestHeader("X-FILE-NAME", file.name);
        ajax.send(formdata);
    }

    function afterUpload()
    {
        // Delay for 3 secs to allow user to see the progress bar
        setTimeout(() => {
            //status_txt = "";
            uploading_now = false;
            form.reset();
            progress_bar.value = 0;
        }, 3000);
    }

    function progressHandler(event)
    {
        uploading_now = true;
        // loaded_total = "Uploaded " + event.loaded + " bytes of " + event.total;
        var percent = (event.loaded / event.total) * 100;
        progress_bar.value = Math.round(percent);
        status_txt = Math.round(percent) + "% uploaded... please wait";
    }

    function completeHandler(event) {
        status_txt = event.target.responseText;
        afterUpload();
    }

    function errorHandler(event) {
        console.log("Upload Failed");
        status_txt = "Upload Failed";
        afterUpload();
    }

    function abortHandler(event) {
        status_txt = "Upload Aborted";
        afterUpload();
    }
</script>


<div class="my-4">
    <div style="display: {uploading_now ? 'none' : 'block'}">
        <form method="post" enctype="multipart/form-data" bind:this="{form}">
            <input type="file" bind:this={input_el} on:change={upload}>
        </form>
    </div>
    <div style="display: {uploading_now ? 'block' : 'none'}">
        <progress bind:this={progress_bar} value="0" max="100" style="width:250px;"></progress>
    </div>
    <div>{status_txt}</div>
</div>


<style>
    input[type="file"] {
    }
</style>