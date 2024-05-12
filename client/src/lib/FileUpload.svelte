<script lang="ts">
import LocalStorageCookies from "@/cookies";
import Dropzone from "svelte-file-dropzone"

let dragActive: boolean = false;
let files = {
    accepted: [] as File[],
    rejected: [] as File[]
};

export let postUrl: string;
// Passed to HTTP POST request:
export let listingData: Object;
export let videoAddedAction: string|undefined;


let progressBar: HTMLProgressElement;
let statusTxt: string = "";
let uploadingNow: boolean = false;
let form: HTMLFormElement;

function afterUpload()
{
    // Delay for 3 secs to allow user to see the progress bar
    setTimeout(() => {
        statusTxt = "";
        uploadingNow = false;
        if (form) { form.reset(); }
        if (progressBar) progressBar.value = 0;
    }, 3000);
}

function progressHandler(event: ProgressEvent<XMLHttpRequestEventTarget>)
{
    uploadingNow = true;
    // loaded_total = "Uploaded " + event.loaded + " bytes of " + event.total;
    var percent = (event.loaded / event.total) * 100;
    if (progressBar) progressBar.value = Math.round(percent);
    statusTxt = Math.round(percent) + "% uploaded... please wait";
}

function completeHandler(event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = (event.target as any).responseText;
    if (progressBar) progressBar.value = 100;
    afterUpload();
}

function errorHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = "Upload Failed";
    afterUpload();
}

function abortHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = "Upload Aborted";
    afterUpload();
}

function upload() {
    for (let i=0; i<files.accepted.length; i++) {
        var file = files.accepted[i];
        var formdata = new FormData();
        formdata.append("fileupload", file);
        var ajax = new XMLHttpRequest();
        statusTxt = "Uploading: " + file.name + "...";
        ajax.upload.addEventListener("progress", progressHandler, false);
        ajax.addEventListener("load", completeHandler, false);
        ajax.addEventListener("error", errorHandler, false) ;
        ajax.addEventListener("abort", abortHandler, false);
        ajax.open("POST", postUrl);
        ajax.setRequestHeader("X-FILE-NAME", file.name);

        let upload_cookies = { ...LocalStorageCookies.getAllNonExpired() };
        if (videoAddedAction)
            upload_cookies["video_added_action"] = videoAddedAction;
        upload_cookies["listing_data_json"] = JSON.stringify(listingData);
        ajax.setRequestHeader("X-CLAPSHOT-COOKIES", JSON.stringify(upload_cookies));

        ajax.send(formdata);
    }
    files.accepted = [];
    files.rejected = [];
}

function onDropFiles(e: any) {
    dragActive = false;
    files.accepted = e.detail.acceptedFiles;
    files.rejected = e.detail.fileRejections;
    if (files.rejected.length > 0 && files.accepted.length==0) {
        alert("Drop rejected. Only video files are allowed.");
    }
    upload();
}
</script>


<div class="w-full h-full inline-block p-0 m-0">
    <div class="w-full h-full" class:display-none={uploadingNow} >
        <Dropzone
            accept={['video/*']}
            disableDefaultStyles={true}
            containerClasses="custom-dropzone {dragActive ? 'drag-active' : ''}"
            containerStyles = "borderColor: '#fff', color: '#90cdf4'"
            inputElement = {undefined}
            on:dragenter={ () => { dragActive = true; }}
            on:dragleave={ () => { dragActive = false; }}
            on:drop={onDropFiles}
        >
          {#if uploadingNow}
            <div class="p-2">
                <progress bind:this={progressBar} value="0" max="100" class="w-[90%] m-2"></progress>
                <div class="text-xs overflow-ellipsis break-words">{statusTxt}</div>
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
    background-color: rgb(15, 23, 42);
    color: #64748b;
    width: 100%;
    height: 100%;

}
:global(.custom-dropzone.drag-active) {
    border-color: #90cdf4;
    color: #9fd0ee;
    background-color: rgb(25, 33, 52);
    transition: background-color 0.1s ease-in-out
}
</style>