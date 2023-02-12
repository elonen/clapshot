<script lang="ts">
    import {slide} from "svelte/transition";
    import Dropzone from "svelte-file-dropzone";

    let drag_active: boolean = false;
    let files = {
        accepted: [],
        rejected: []
    };
    function dropzoneHandleFilesSelect(e) {
        drag_active = false;
        files.rejected = [];    // Clear old rejected files
        const { acceptedFiles, fileRejections } = e.detail;
        files.accepted = [...files.accepted, ...acceptedFiles];
        files.rejected = [...files.rejected, ...fileRejections];

        // Remove duplicates
        files.accepted = files.accepted.filter((file, index, self) =>
            index === self.findIndex((f) => (
                f.name === file.name && f.size === file.size
            ))
        );

        // Scroll to bottom
        setTimeout(() => {
            let el = document.getElementById("end-elem");
            if (el) { el.scrollIntoView({behavior: "smooth", block: "end", inline: "nearest"}); }
        }, 100);
    }
    function removeFileName(file) {
        files.accepted = files.accepted.filter(f => f !== file);
    }

    export let post_url: string;

    let progress_bar: HTMLProgressElement;
    let status_txt: string = "";
    let uploading_now: boolean = false;
    let form: HTMLFormElement = null;

    function upload()
    {
        for (let i = 0; i < files.accepted.length; i++) {
            var file = files.accepted[i];
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
        files.accepted = [];
        files.rejected = [];
    }

    function afterUpload()
    {
        // Delay for 3 secs to allow user to see the progress bar
        setTimeout(() => {
            //status_txt = "";
            uploading_now = false;
            if (form) { form.reset(); }
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
        <Dropzone on:drop={dropzoneHandleFilesSelect}
            accept={['video/*']}
            disableDefaultStyles="true"
            containerClasses="custom-dropzone {drag_active ? 'drag-active' : ''}"
            containerStyles = "borderColor: '#fff', color: '#90cdf4'"
            on:dragenter={ () => { drag_active = true; }}
            on:dragleave={ () => { drag_active = false; }}
        >
                <button>Drag and drop videos here or click to browse</button>
        </Dropzone>
        <ol class="mt-2">
          {#each files.accepted as item}
            <li class="font-mono" transition:slide>{item.name}
                <span class="font-sans text-gray-500">({ (item.size/1024/1024).toFixed(1) } MB)</span>
                <button on:click|stopPropagation={ () => removeFileName(item) }>
                    <i class="text-red-800 fa-solid fa-x hover:text-red-600"></i>
                </button>
            </li>
          {/each}
          {#each files.rejected as item}
            <li class="font-mono text-gray-500 line-through">{item.file.name}</li>
          {/each}
          {#if files.accepted.length > 0}
            <li>
                <button class="bg-slate-700 hover:bg-slate-500 py-1 px-2 my-2 rounded"
                    on:click|stopPropagation={upload}>
                    Upload
                </button>
            </li>
          {/if}
        </ol>
    </div>
    <div style="display: {uploading_now ? 'block' : 'none'}">
        <progress bind:this={progress_bar} value="0" max="100" style="width:250px;"></progress>
    </div>
    <div>{status_txt}</div>
</div>
<span id="end-elem"></span>


<style>
    :global(.custom-dropzone) {
        border: 0.2em dashed #64748b;
        border-radius: 0.5em;
        padding: 1.5em;
        text-align: center;
        background-color: rgb(17,24,39);
        color: #64748b;
    }
    :global(.custom-dropzone.drag-active) {
        border-color: #90cdf4;
        color: #90cdf4;
        background-color: rgb(26, 36, 59);
        transition: background-color 0.3s ease-in-out
    }
</style>