<script lang="ts">
/**
 * Thumbnail that lets you scrub through the video
 * by hovering over it and moving the mouse.
 *
 * Uses a sprite sheet to show the frames.
 * When the mouse is not over the thumbnail,
 * a single poster image is shown instead.
 */
export let thumbPosterUrl: string;
export let thumbSheetUrl: string | null = null;
export let thumbSheetCols: number | null = null;
export let thumbSheetRows: number | null = null;

export let extra_styles: string = "";

function installThumbScrubber(e: MouseEvent)
{
    if (thumbSheetUrl == null || thumbSheetCols == null || thumbSheetRows == null)
        return;

    let el = e.target as HTMLElement;
    let sheetCols = thumbSheetCols;
    let sheetRows = thumbSheetRows;
    let bgImg =  new Image();

    bgImg.onload = (_e) => {
        // Total size of sprite sheet in pixels
        let sheet_w_px = bgImg.naturalWidth;
        let sheet_h_px = bgImg.naturalHeight;

        // Size of one frame in pixels
        let frame_width = sheet_w_px / sheetCols;
        let frame_height = sheet_h_px / sheetRows;

        // Size of current div (that shows the sprite sheet) in pixels
        let div_w_px = el.clientWidth;
        let div_h_px = el.clientHeight;

        // Switch background image to the now loaded sprite sheet
        el.style.backgroundRepeat = 'no-repeat';
        el.style.backgroundImage = 'url(' + bgImg.src + ')';

        // Scale the sprite sheet so one frame fits in the div
        let scaled_bgr_w = (div_w_px / frame_width) * sheet_w_px;
        let scaled_bgr_h = (div_h_px / frame_height) * sheet_h_px;
        el.style.backgroundSize = scaled_bgr_w + 'px ' + scaled_bgr_h + 'px';

        function show_frame(frame_idx: number) {
            let frame_xi = frame_idx % sheetCols;
            let frame_yi = Math.floor(frame_idx / sheetCols);

            let frame_left = scaled_bgr_w * (frame_xi / sheetCols);
            let frame_top = scaled_bgr_h * (frame_yi / sheetRows);

            el.style.backgroundPosition = '-' + frame_left + 'px -' + frame_top + 'px';
        }
        show_frame(0);

        // Scrub sheet on mouse move
        el.onmousemove = (e: MouseEvent) => {
            let frame_idx = Math.floor((e.offsetX / el.clientWidth) * (sheetCols * sheetRows));
            show_frame(frame_idx);
        }
    };
    // Start loading the sprite sheet
    bgImg.src = thumbSheetUrl;
}

function removeThumbScrubber(e: MouseEvent)
{
    // Restore original background image (item.thumb_url)
    let el = e.target as HTMLElement;
    el.onmousemove = null;
    el.onload = null;
    el.style.backgroundImage = 'url(' + thumbPosterUrl + ')';
    el.style.backgroundPosition = '0 0';
    el.style.backgroundSize = '100% 100%';
}
</script>

<div class="w-full aspect-video mx-auto rounded-md overflow-hidden"
  style="background-image: url('{thumbPosterUrl}'); background-size: cover; background-position: 0 0; {extra_styles}"
  role="img"
  on:blur={()=>{}}
  on:focus={()=>{}}
  on:mouseover={installThumbScrubber}
  on:mouseout={removeThumbScrubber}
></div>
