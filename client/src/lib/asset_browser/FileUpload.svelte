<script lang="ts">
import LocalStorageCookies from "@/cookies";
import Dropzone from "svelte-file-dropzone"
import { t } from "@/i18n";

let dragActive: boolean = $state(false);
let files = {
    accepted: [] as File[],
    rejected: [] as File[]
};


    interface Props {
        postUrl: string;
        // Passed to HTTP POST request:
        listingData: Object;
        mediaFileAddedAction: string|undefined;
        children?: import('svelte').Snippet;
    }

    let {
        postUrl,
        listingData,
        mediaFileAddedAction,
        children
    }: Props = $props();


let progressBar: HTMLProgressElement | undefined = $state();
let statusTxt: string = $state("");
let uploadingNow: boolean = $state(false);
let form: HTMLFormElement | undefined;

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
    statusTxt = $t('upload.progress', { percent: Math.round(percent) });
}

function completeHandler(event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = (event.target as any).responseText;
    if (progressBar) progressBar.value = 100;
    afterUpload();
}

function errorHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = $t('upload.failed');
    afterUpload();
}

function abortHandler(_event: ProgressEvent<XMLHttpRequestEventTarget>) {
    statusTxt = $t('upload.aborted');
    afterUpload();
}

function upload() {
    for (let i=0; i<files.accepted.length; i++) {
        var file = files.accepted[i];
        var formdata = new FormData();
        formdata.append("fileupload", file);
        var ajax = new XMLHttpRequest();
        statusTxt = $t('upload.uploading', { filename: file.name });
        ajax.upload.addEventListener("progress", progressHandler, false);
        ajax.addEventListener("load", completeHandler, false);
        ajax.addEventListener("error", errorHandler, false) ;
        ajax.addEventListener("abort", abortHandler, false);
        ajax.open("POST", postUrl);
        ajax.setRequestHeader("X-FILE-NAME", encodeURIComponent(file.name));

        let upload_cookies = { ...LocalStorageCookies.getAllNonExpired() };
        if (mediaFileAddedAction)
            upload_cookies["media_file_added_action"] = mediaFileAddedAction;
        upload_cookies["listing_data_json"] = JSON.stringify(listingData);
        ajax.setRequestHeader("X-CLAPSHOT-COOKIES", JSON.stringify(upload_cookies));

        ajax.send(formdata);
    }
    files.accepted = [];
    files.rejected = [];
}

function onDropFiles(e: any) {
    dragActive = false;
    files.accepted = e.detail.acceptedFiles || [];
    files.rejected = e.detail.fileRejections || [];
    if (files.rejected.length > 0 && files.accepted.length == 0) {
        alert($t('upload.rejected'));
    }
    upload();
}
</script>


<div class="w-full h-full inline-block p-0 m-0">
    <div class="w-full h-full" class:display-none={uploadingNow} >
        <Dropzone
            accept={['video/*', 'image/*', 'audio/*']}
            disableDefaultStyles={true}
            containerClasses="custom-dropzone {dragActive ? 'drag-active' : ''}"
            containerStyles = "borderColor: '#fff', color: '#90cdf4'"
            on:drop={onDropFiles}
            on:dragenter={() => { dragActive = true; }}
            on:dragleave={() => { dragActive = false; }}
        >
          {#if uploadingNow}
            <div class="p-2">
                <progress bind:this={progressBar} value="0" max="100" class="w-[90%] m-2"></progress>
                <div class="text-xs overflow-ellipsis break-words">{statusTxt}</div>
            </div>
          {:else}
            {@render children?.()}
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
