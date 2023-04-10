<script lang="ts">
    import Dropzone from "svelte-file-dropzone/Dropzone.svelte";

    let drag_active: boolean = false;
    let files = {
        accepted: [] as File[],
        rejected: [] as File[]
    };

    export let post_url: string;

    let progress_bar: HTMLProgressElement;
    let status_txt: string = "";
    let uploading_now: boolean = false;
    let form: HTMLFormElement;

    function afterUpload()
    {
        // Delay for 3 secs to allow user to see the progress bar
        setTimeout(() => {
            status_txt = "";
            uploading_now = false;
            if (form) { form.reset(); }
            progress_bar.value = 0;
        }, 3000);
    }

    function progressHandler(event: ProgressEvent<XMLHttpRequestEventTarget>)
    {
        uploading_now = true;
        // loaded_total = "Uploaded " + event.loaded + " bytes of " + event.total;
        var percent = (event.loaded / event.total) * 100;
        progress_bar.value = Math.round(percent);
        status_txt = Math.round(percent) + "% uploaded... please wait";
    }

    function completeHandler(event: ProgressEvent<XMLHttpRequestEventTarget>) {
        status_txt = (event.target as any).responseText;
        progress_bar.value = 100;
        afterUpload();
    }

    function errorHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
        status_txt = "Upload Failed";
        afterUpload();
    }

    function abortHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
        status_txt = "Upload Aborted";
        afterUpload();
    }

    function upload() {
        for (let i=0; i<files.accepted.length; i++) {
            var file = files.accepted[i];
            var formdata = new FormData();
            formdata.append("fileupload", file);
            var ajax = new XMLHttpRequest();
            status_txt = "Uploading: " + file.name + "...";
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

    function onDropFiles(e: any) {
        drag_active = false;
        files.accepted = e.detail.acceptedFiles;
        files.rejected = e.detail.fileRejections;
        if (files.rejected.length > 0 && files.accepted.length==0) {
            alert("Drop rejected. Only video files are allowed.");
        }
        upload();
    }

</script>


<div class="w-full h-full inline-block p-0 m-0">
    <div class="w-full h-full" class:display-none={uploading_now} >
        <Dropzone
            accept={['video/*']}
            disableDefaultStyles={true}
            containerClasses="custom-dropzone {drag_active ? 'drag-active' : ''}"
            containerStyles = "borderColor: '#fff', color: '#90cdf4'"
            inputElement = {undefined}
            on:dragenter={ () => { drag_active = true; }}
            on:dragleave={ () => { drag_active = false; }}
            on:drop={onDropFiles}
        >
          {#if uploading_now}
            <div class="p-2">
                <progress bind:this={progress_bar} value="0" max="100" class="w-[90%] m-2"></progress>
                <div class="text-xs overflow-ellipsis break-words">{status_txt}</div>
            </div>
          {:else}
            <slot></slot>
          {/if}
        </Dropzone>
    </div>
</div>


<style>
    :global(.custom-dropzone) {
        text-align: center;
        background-color: rgb(17,24,39);
        color: #64748b;
        width: 100%;
        height: 100%;

    }
    :global(.custom-dropzone.drag-active) {
        border-color: #90cdf4;
        color: #90cdf4;
        background-color: rgb(38, 52, 86);
        transition: background-color 0.1s ease-in-out
    }
</style>