<script lang="ts">
import { curVideo, allComments } from "@/stores";
import { Modal, Button, Input, Label, Select } from 'flowbite-svelte';
import { onMount } from "svelte";
import { exporters, groupComments, downloadFile, type ExportContext } from './index';

interface Props {
    isOpen?: boolean;
}

let { isOpen = $bindable(false) }: Props = $props();

let selectedExporterId = $state(exporters[0]?.id || '');
let optionValues = $state<Record<string, string | number | boolean>>({});
let frameRate: number = $state(24);

// Get selected exporter
let selectedExporter = $derived(exporters.find(e => e.id === selectedExporterId));

// Group comments for export
let groupedComments = $derived(groupComments($allComments));

// Read fps from current video on mount
onMount(() => {
    frameRate = parseFloat($curVideo?.duration?.fps || "24");
    if (frameRate <= 0 || isNaN(frameRate)) { frameRate = 24; }
});

// Reset option values when exporter changes
$effect(() => {
    if (selectedExporter) {
        const defaults: Record<string, string | number | boolean> = {};
        for (const opt of selectedExporter.options) {
            defaults[opt.id] = opt.default;
        }
        optionValues = defaults;
    }
});

function handleExport() {
    if (!selectedExporter || groupedComments.length === 0) return;

    const ctx: ExportContext = {
        comments: groupedComments,
        title: $curVideo?.title || $curVideo?.id || "Clapshot Export",
        fps: frameRate,
        duration: $curVideo?.duration?.duration,
    };

    // Add frameRate to options if exporter uses it
    const opts = { ...optionValues, frameRate };

    const content = selectedExporter.export(ctx, opts);
    const filename = ($curVideo?.id || "comments") + selectedExporter.extension;
    const mimeType = selectedExporter.extension === '.xml' ? 'application/xml' :
                     selectedExporter.extension === '.json' || selectedExporter.extension === '.otrn' ? 'application/json' :
                     'text/plain';

    downloadFile(content, filename, mimeType);
    isOpen = false;
}

function setOptionValue(id: string, value: string | number | boolean) {
    optionValues = { ...optionValues, [id]: value };
}

// Format dropdown items
let formatItems = $derived(exporters.map(e => ({ value: e.id, name: e.name })));
</script>

<Modal title="Export Comments" bind:open={isOpen} class="w-96">
    <div class="flex flex-col space-y-4">
        <!-- Format selection -->
        <div>
            <Label for="format_select">Export format</Label>
            <Select id="format_select" items={formatItems} bind:value={selectedExporterId} class="mt-1" />
        </div>

        <!-- Frame rate (common to most formats) -->
        <div>
            <Label for="fps_export">Frame rate</Label>
            <Input id="fps_export" type="number" bind:value={frameRate} class="mt-1" />
        </div>

        <!-- Dynamic options based on selected exporter -->
        {#if selectedExporter && selectedExporter.options.length > 0}
            <div class="border-t border-gray-600 pt-4">
                <h3 class="text-sm font-medium text-gray-400 mb-2">Format options</h3>
                {#each selectedExporter.options as opt}
                    <div class="mb-3">
                        {#if opt.type === 'number'}
                            <Label for={opt.id}>{opt.label}</Label>
                            <input
                                id={opt.id}
                                type="number"
                                value={optionValues[opt.id] ?? opt.default}
                                oninput={(e) => setOptionValue(opt.id, parseFloat(e.currentTarget.value) || 0)}
                                class="mt-1 block w-full rounded-lg border border-gray-600 bg-gray-700 p-2.5 text-sm text-white focus:border-blue-500 focus:ring-blue-500"
                            />
                        {:else if opt.type === 'string'}
                            <Label for={opt.id}>{opt.label}</Label>
                            <input
                                id={opt.id}
                                type="text"
                                value={optionValues[opt.id] ?? opt.default}
                                oninput={(e) => setOptionValue(opt.id, e.currentTarget.value)}
                                placeholder={String(opt.default) || ''}
                                class="mt-1 block w-full rounded-lg border border-gray-600 bg-gray-700 p-2.5 text-sm text-white focus:border-blue-500 focus:ring-blue-500"
                            />
                        {:else if opt.type === 'boolean'}
                            <label class="flex items-center gap-2 cursor-pointer">
                                <input
                                    id={opt.id}
                                    type="checkbox"
                                    checked={Boolean(optionValues[opt.id] ?? opt.default)}
                                    onchange={(e) => setOptionValue(opt.id, e.currentTarget.checked)}
                                    class="h-4 w-4 rounded border-gray-600 bg-gray-700 text-blue-600 focus:ring-blue-500"
                                />
                                <span class="text-sm text-white">{opt.label}</span>
                            </label>
                        {:else if opt.type === 'select' && opt.choices}
                            <Label for={opt.id}>{opt.label}</Label>
                            <select
                                id={opt.id}
                                value={optionValues[opt.id] ?? opt.default}
                                onchange={(e) => setOptionValue(opt.id, e.currentTarget.value)}
                                class="mt-1 block w-full rounded-lg border border-gray-600 bg-gray-700 p-2.5 text-sm text-white focus:border-blue-500 focus:ring-blue-500"
                            >
                                {#each opt.choices as choice}
                                    <option value={choice.value}>{choice.label}</option>
                                {/each}
                            </select>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}
    </div>

    <!-- Action buttons -->
    <div class="flex gap-2 mt-4">
        {#if groupedComments.length > 0}
            <Button onclick={handleExport} color="primary">Export</Button>
        {/if}
        <Button onclick={() => {isOpen = false;}} color="alternative">Cancel</Button>
    </div>

    <!-- Comments summary -->
    {#if groupedComments.length > 0}
        <p class="mt-4 text-sm text-gray-400">
            {groupedComments.length} comment{groupedComments.length !== 1 ? 's' : ''} will be exported.
        </p>
    {:else}
        <p class="mt-4 text-gray-400">No comments to export.</p>
    {/if}
</Modal>
