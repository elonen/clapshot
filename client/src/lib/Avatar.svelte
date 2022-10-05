<!--
This is all derivative of the codepen and gist linked below. I claim none of it except the structure of the component.
Any work in here that is original enough to need a license is offered under the MIT license:
https://github.com/IQAndreas/markdown-licenses/blob/master/mit.md
-->
<script>
    export let userFullName;
    export let avatarColors = [
        "#1abc9c",
        "#2ecc71",
        "#3498db",
        "#9b59b6",
        "#34495e",
        "#16a085",
        "#27ae60",
        "#2980b9",
        "#8e44ad",
        "#2c3e50",
        "#f1c40f",
        "#e67e22",
        "#e74c3c",
        "#ecf0f1",
        "#95a5a6",
        "#f39c12",
        "#d35400",
        "#c0392b",
        "#bdc3c7",
        "#7f8c8d",
    ];
    import { onMount } from "svelte";
    export let width = 32;
    export let round = true;
    export let src = null;
    let avatarImage;

    /*
     * LetterAvatar
     *
     * Artur Heinze
     * Create Letter avatar based on Initials
     * based on https://gist.github.com/leecrossley/6027780
     * 
     * This component is based on https://codepen.io/arturheinze/pen/ZGvOMw
     */
    function LetterAvatar(name, size) {
        name = name || "";
        size = size || 60;

        var colours = avatarColors,
            nameSplit = String(name).toUpperCase().split(" "),
            initials,
            colourIndex,
            canvas,
            context;

        if (nameSplit.length == 1) {
            initials = nameSplit[0] ? nameSplit[0].charAt(0) : "?";
        } else {
            initials = nameSplit[0].charAt(0) + nameSplit[1].charAt(0);
        }

        if (window.devicePixelRatio)
            size = size * window.devicePixelRatio;  // In case display zoomed or retina display

        let checksum = 0;
        for (let i=0; i<name.length; i++)
            checksum += name.charCodeAt(i)*i;

        colourIndex = checksum % 20;
        canvas = document.createElement("canvas");
        canvas.width = size;
        canvas.height = size;
        context = canvas.getContext("2d");

        context.fillStyle = colours[colourIndex - 1];
        context.fillRect(0, 0, canvas.width, canvas.height);
        context.font = "bold " + Math.round(canvas.width / 2.2) + "px Arial";
        context.font
        context.textAlign = "center";
        context.fillStyle = "#222";
        context.fillText(initials, size / 2, size / 1.5);

        let dataURI = canvas.toDataURL();
        canvas = null;

        return dataURI;
    }

    onMount(() => {
        avatarImage.src = (src && (src!=="")) ? src : LetterAvatar(userFullName, width);
    });
</script>

<img bind:this={avatarImage} class:round={round} loading="lazy" alt={userFullName} />

<style>
    .round {
        border-radius: 50%;
        filter: drop-shadow(2px 2px 2px rgba(0,0,0,0.3));
    }
</style>
