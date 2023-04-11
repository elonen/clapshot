<script context="module" lang="ts">

// Convert a username to a hex color
export function hexColorForUsername(name: string): string {
    const AVATAR_COLORS = [
        "#1abc9c",
        "#2ecc71",
        "#3498db",
        "#9b59b6",
        "#16a085",
        "#27ae60",
        "#2980b9",
        "#8e44ad",
        "#f1c40f",
        "#e67e22",
        "#e74c3c",
        "#f39c12",
        "#d35400",
        "#c0392b",
        "#bdc3c7",
    ];
    let checksum = 0;
    for (let i=0; i<name.length; i++)
        checksum += name.charCodeAt(i)*i;
    let res = AVATAR_COLORS[checksum % AVATAR_COLORS.length];
    return res;
}

</script>

<script lang="ts">
import { onMount } from "svelte";

export let username: string | null = "";
export let width = "32";
export let round = true;
export let src: string | null = null;

/*
    * LetterAvatar. Based on https://codepen.io/arturheinze/pen/ZGvOMw, which is based on https://gist.github.com/leecrossley/6027780
    */
function MakeLetterAvatar(name: string | null, size: number): string {
    name = name || "?";
    size = size || 60;

    let initials: string;
    let canvas: HTMLCanvasElement = document.createElement("canvas");
    let context: CanvasRenderingContext2D = canvas.getContext("2d") ?? new CanvasRenderingContext2D();

    let nameSplit = String(name).toUpperCase().replace(".", " ").split(" ");
    if (nameSplit.length == 1) {
        initials = nameSplit[0] ? nameSplit[0].charAt(0) : "?";
    } else {
        initials = nameSplit[0].charAt(0) + nameSplit[1].charAt(0);
    }

    if (window.devicePixelRatio)
        size = size * window.devicePixelRatio;  // In case display zoomed or retina display
    canvas.width = size;
    canvas.height = size;

    context.fillStyle = hexColorForUsername(name);
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.font = "bold " + Math.round(canvas.width / 2.2) + "px Arial";
    context.textAlign = "center";
    context.fillStyle = "#222";
    context.fillText(initials, size / 2, size / 1.5);

    return canvas.toDataURL();  // Base64 encoded data url string + colour hex
}

const letterAvatar = MakeLetterAvatar(username, parseFloat(width));
let avatarImage: HTMLImageElement;
onMount(() => {
    avatarImage.src = (src && (src!=="")) ? src : letterAvatar;
});
</script>

<img bind:this={avatarImage} class:round={round} loading="lazy" alt={username} />

<style>
    .round {
        border-radius: 50%;
        filter: drop-shadow(2px 2px 2px rgba(0,0,0,0.3));
    }
</style>
