<script lang="ts">
import { curVideo } from "@/stores";
import { Modal, Button, Input, Fileupload, Label, Helper } from 'flowbite-svelte';
import { createEventDispatcher, onMount } from "svelte";
import * as Proto3 from '@clapshot_protobuf/typescript';

const dispatch = createEventDispatcher();

export let isOpen: boolean = false;

let edlForm: HTMLFormElement;
let frameRate: number = 24;

// Read fps from current video
onMount(() => {
    console.debug("EDLImport: Current video: ", $curVideo);
    frameRate = parseFloat($curVideo?.duration?.fps || "24");
    console.debug("EDLImport: Frame rate from video: ", frameRate);
    if (frameRate <= 0 || isNaN(frameRate)) { frameRate = 24; }
});


let edlEvents: EDLEvent[] = [];
let errorMsg: string|null = null;


// --- Simplistic EDL parser ---

type EDLEvent = {
    eventNumber: string;
    sourceIn: string;
    sourceOut: string;
    recordIn: string;
    recordOut: string;
    fromClipName?: string;
};

function parseEDL(edlText: string): EDLEvent[] {
    const events: EDLEvent[] = [];
    const lines = edlText.split('\n');
    let currentEvent: Partial<EDLEvent> | null = null;

    lines.forEach(line => {
        // Match event lines that start with a number and have four timecodes
        const eventMatch = line.match(/^(\d+).*(\d{2}:\d{2}:\d{2}:\d{2})\s+(\d{2}:\d{2}:\d{2}:\d{2})\s+(\d{2}:\d{2}:\d{2}:\d{2})\s+(\d{2}:\d{2}:\d{2}:\d{2})/);
        if (eventMatch) {
            if (currentEvent) {
                events.push(currentEvent as EDLEvent);
            }
            currentEvent = {
                eventNumber: eventMatch[1],
                sourceIn: eventMatch[2],
                sourceOut: eventMatch[3],
                recordIn: eventMatch[4],
                recordOut: eventMatch[5],
            };
        }
        // Match lines containing "FROM CLIP NAME"
        if (currentEvent) {
            const fromClipNameMatch = line.match(/^.*FROM CLIP NAME:\s+(.+)$/);
            if (fromClipNameMatch) {
                currentEvent.fromClipName = fromClipNameMatch[1].trim();
            }
        }
    });
    if (currentEvent) { events.push(currentEvent as EDLEvent); }
    return events;
}



// --- Form handling ---

function handleFileUpload(event: Event) {
    const files = (event.target as HTMLInputElement).files;
    if (files) {
        const reader = new FileReader();
        reader.onload = function() {
            const text = reader.result as string;
            edlEvents = parseEDL(text);
            if (!edlEvents || edlEvents.length === 0) { errorMsg = "No time spans found when parsing EDL."; }
        };
        reader.readAsText(files[0]);
    }
};

const handleAccept = () => {
    if (edlEvents.length > 0) {
        let comments: Proto3.Comment[] = edlEvents.map(edle => {
            return {
                mediaFileId: $curVideo?.id,
                comment: "EDL (" + (edle.fromClipName || edle.eventNumber || "") + ")",
                timecode: edle.recordIn,
            } as Proto3.Comment;
        });
        dispatch('add-comments', comments);
        isOpen = false;
    }
};
</script>

<Modal title="Import EDL as Comments" bind:open={isOpen} class="w-96">
    <form bind:this={edlForm} class="flex flex-col space-y-1" action="#">
        <Label for="file_up">Upload EDL</Label>
        <Fileupload id="file_up" accept=".edl" on:change={handleFileUpload} />
        <Label for="fps_input" class="pt-2">Frame rate</Label>
        <Input id="fps_input" type="number" bind:value={frameRate}/>
    </form>
    <svelte:fragment slot="footer">
        {#if edlEvents.length>0}
            <Button on:click={handleAccept} color="primary">Add as comments</Button>
        {/if}
        <Button on:click={() => {isOpen=false;}} color="alternative">Cancel</Button>
    </svelte:fragment>

    <!-- scrollable list of time spans for review -->
    {#if edlEvents.length > 0}
        <h2>Parsed spans</h2>
        <ul class="space-y-2 max-h-32 overflow-scroll bg-gray-700 p-1">
            {#each edlEvents as edle}
                <li>{edle.recordIn}: {edle.fromClipName || edle.eventNumber}</li>
            {/each}
        </ul>
    {:else if errorMsg}
        <Helper color="red" class="text-lg">{errorMsg}</Helper>
    {/if}
</Modal>
