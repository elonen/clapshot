<script lang="ts">
    /**
     * Thumbnail that lets you scrub through the video
     * by hovering over it and moving the mouse.
     * 
     * Uses a sprite sheet to show the frames.
     * When the mouse is not over the thumbnail,
     * a single poster image is shown instead.
     */

    export let thumb_poster_url: string;
    export let thumb_sheet_url: string;
    export let thumb_sheet_cols: number;
    export let thumb_sheet_rows: number;

    export let extra_styles: string = "";

    function installThumbScrubber(e: MouseEvent)
    {
        let thumb_el = e.target as HTMLElement;

        let sheet_cols = thumb_sheet_cols;
        let sheet_rows = thumb_sheet_rows;
        let bgImg =  new Image();
        
        bgImg.onload = (_e) => {
            // Total size of sprite sheet in pixels
            let sheet_w_px = bgImg.naturalWidth;
            let sheet_h_px = bgImg.naturalHeight;

            // Size of one frame in pixels
            let frame_width = sheet_w_px / sheet_cols;
            let frame_height = sheet_h_px / sheet_rows;

            // Size of current div (that shows the sprite sheet) in pixels
            let div_w_px = thumb_el.clientWidth;
            let div_h_px = thumb_el.clientHeight;

            // Switch background image to the now loaded sprite sheet
            thumb_el.style.backgroundRepeat = 'no-repeat';
            thumb_el.style.backgroundImage = 'url(' + bgImg.src + ')';

            // Scale the sprite sheet so one frame fits in the div
            let scaled_bgr_w = (div_w_px / frame_width) * sheet_w_px;
            let scaled_bgr_h = (div_h_px / frame_height) * sheet_h_px;
            thumb_el.style.backgroundSize = scaled_bgr_w + 'px ' + scaled_bgr_h + 'px';

            function show_frame(frame_idx: number) {
                let frame_xi = frame_idx % sheet_cols;
                let frame_yi = Math.floor(frame_idx / sheet_cols);

                let frame_left = scaled_bgr_w * (frame_xi / sheet_cols);
                let frame_top = scaled_bgr_h * (frame_yi / sheet_rows);

                thumb_el.style.backgroundPosition = '-' + frame_left + 'px -' + frame_top + 'px';
            }
            show_frame(0);

            // Scrub sheet on mouse move
            thumb_el.onmousemove = (e: MouseEvent) => {
                let frame_idx = Math.floor((e.offsetX / thumb_el.clientWidth) * (sheet_cols * sheet_rows));
                show_frame(frame_idx);
            }
        };
        // Start loading the sprite sheet
        bgImg.src = thumb_sheet_url;
    }

    function removeThumbScrubber(e: MouseEvent)
    {
        // Restore original background image (item.thumb_url)
        let thumb_el = e.target as HTMLElement;
        thumb_el.onmousemove = null;
        thumb_el.onload = null;
        thumb_el.style.backgroundImage = 'url(' + thumb_poster_url + ')';
        thumb_el.style.backgroundPosition = '0 0';
        thumb_el.style.backgroundSize = '100% 100%';
    }
</script>

<div class="w-full aspect-video mx-auto rounded-md overflow-hidden"
  style="background-image: url('{thumb_poster_url}'); background-size: cover; background-position: 0 0; {extra_styles}"
  on:blur={()=>{}}
  on:focus={()=>{}}
  on:mouseover={installThumbScrubber}
  on:mouseout={removeThumbScrubber}
></div>
