# This jq (JSON Query) script parses `mediainfo` output (+args), detects media type and
# generates ffmpeg options for transcoding, thumbnail and thumbsheet creation.
#
# Clapshot Server executes this script to determine how to process uploaded media files.
#
# Example usage:
# mediainfo 60fps-example.mp4 --output=JSON | jq '{"total_frames": 123, "clapshot_vars": {"max_video_bitrate": 2500}, "mediainfo": .}' | jq -f media-detect.jq
#
# Input:
# {
#    "mediainfo": <JSON from `mediainfo --Output=JSON`>,
#    "total_frames": <total number of frames in the video> | null,      # only for video files, from ffprobe (since metadata might be inaccurate)
#    "file_size": <original file size in bytes> | null,
#    "clapshot_vars": {
#        "max_video_bitrate": <maximum video bitrate for transcoding>    # video bitrate limiter, in bits per second
#    }
# }
#
# Returns a JSON object with the following structure:
# {
#    "error": null | "<error message>",
#    "media_type": "video" | "audio" | "image",
#    "transcode": null | {     # null if no transcoding needed
#        "reason": "<explanation on why transcoding was needed>",
#        "ffmpeg_options": ["-map", "0", "-dn", "-vcodec", "libx264", ...]
#    },
#    "ffmpeg_thumbnail_options": ["-vcodec", "libwebp", ...],
#    "ffmpeg_thumbsheet_options": ["-vf", "select=...", ...],
# }
#
# Notes:
# - FFmpeg options will be added between the `ffmpeg` command input and output arguments,
#   and may also be amended with additional options for progress reporting, file rewrite etc.
# - If "error" is not null, the rest of the fields will be ignored (even if present) by the caller
# - Thumbnails and thumbsheets will always be executed on transcoded video file when "transcode" is not null, original otherwise
# - If multiple video or image tracks are present, the first one of each will be used for processing


# --- Video thumbsheet ---
# (Thumbsheet is a grid of thumbnails from the video. It is used as a preview for the video.)

def ffmpeg_thumbsheet_frame_select_filter($thumbs; $total_frames):
  # Create a filter that selects exactly $thumbs frames from the video
  # E.g.: "eq(n,0)+eq(n,16)+eq(n,32)+eq(n,48)+eq(n,64)+eq(n,80)+eq(n,96)+eq(n,112)"
  [
    range(0; $thumbs)
    | . as $pos
    | ($pos * ($total_frames / $thumbs) | floor)
    | "eq(n,\(.))"
  ]
  | join("+");


def ffmpeg_thumbsheet_frame_reshape($thumb_w; $thumb_h):
    "scale=\($thumb_w):\($thumb_h):force_original_aspect_ratio=decrease,pad=\($thumb_w):\($thumb_h):(ow-iw)/2:(oh-ih)/2";


def ffmpeg_thumbsheet_options($total_frames; $thumb_w; $thumb_h; $thumb_sheet_cols; $thumb_sheet_rows):
    [
        "-nostats",
        "-vf", ("select=" + ffmpeg_thumbsheet_frame_select_filter($thumb_sheet_cols * $thumb_sheet_rows; $total_frames) +
            "," + ffmpeg_thumbsheet_frame_reshape($thumb_w; $thumb_h) +
            ",tile=\($thumb_sheet_cols)x\($thumb_sheet_rows)"),
        "-strict", "experimental",
        "-c:v", "libwebp",
        "-vsync", "vfr",
        "-start_number", "0"
    ];


# --- Video thumbnail ---
# (Thumbnail is a single "cover image" frame from the video.)

def ffmpeg_thumbnail_options($thumb_w; $thumb_h):
    [
        "-nostats",
        "-vcodec", "libwebp",
        "-vf", ("thumbnail,scale=\($thumb_w):\($thumb_h):force_original_aspect_ratio=decrease,pad=\($thumb_w):\($thumb_h):(ow-iw)/2:(oh-ih)/2"),
        "-frames:v", "1",
        "-strict", "experimental",
        "-c:v", "libwebp"
    ];

# --- Video transcoding ---
# (Some videos need to be transcoded to a different format or bitrate for browser compatibility and performance reasons.)

def ffmpeg_transcode_options($video_bitrate):
    [
        "-map", "0",
        "-dn",
        "-vcodec", "libx264",
        "-vf", "scale=1920:-8",
        "-preset", "faster",
        "-acodec", "aac",
        "-ac", "2",
        "-strict", "experimental",
        "-b:v", ($video_bitrate | tostring),
        "-b:a", "128000"
    ];

# Decide if transcoding is needed, and if so, what options to use
def video_decide_transcoding($metadata; $max_video_bitrate):
    $metadata |
    if (.media.track[] | select(."@type" == "General") | .FileExtension | test("mp4|mkv"; "i")) then
        if (.media.track[] | select(."@type" == "Video") | .Format | ascii_downcase | test("h264|avc|hevc|h265")) then
            if ((.media.track[] | select(."@type" == "Video") | .BitRate | tonumber) <= ($max_video_bitrate * 1.2)) then
                null
            else
                (.media.track[] | select(."@type" == "Video") | .BitRate | tonumber | if . < $max_video_bitrate then . else $max_video_bitrate end)
                    as $new_bitrate
                | {"reason": "bitrate too high", "ffmpeg_options": ffmpeg_transcode_options($new_bitrate)}
            end
        else
            {"reason": "codec not supported", "ffmpeg_options": ffmpeg_transcode_options($max_video_bitrate)}
        end
    else
        {"reason": "container not supported", "ffmpeg_options": ffmpeg_transcode_options($max_video_bitrate)}
    end;


# --- Audio transcoding ---

# Transcode audio to video with rich waveform visualization, 1920x1080 total, 60fps
def audio_to_video_transcoding($video_bitrate):
    [
        "-dn",
        "-r", "60",
        "-filter_complex",
            "color=c=white:s=2x720 [cursor]; " +
            "[0:a] showwavespic=s=1920x720:split_channels=1:draw=full, fps=60 [stillwave]; " +
            "[0:a] showfreqs=mode=line:ascale=log:s=1920x180 [freqwave]; " +
            "[0:a] showwaves=size=1920x180:mode=p2p [livewave]; " +
            "[stillwave][cursor] overlay=(W*t)/%%DURATION%%:0:shortest=1 [progress]; " +
            "[livewave][progress] vstack[stacked]; " +
            "[stacked][freqwave] vstack [out];",
        "-map", "[out]",
        "-map", "0:a",
        "-strict", "experimental",
        "-vcodec", "libx264",
        "-b:v", ($video_bitrate | tostring),
        "-acodec", "flac"
    ];

# --- Image transcoding ---

# Transcode image to 1s video with max 1920 width, 30fps, keep aspect ratio
def image_to_video_transcoding($video_bitrate):
    [
        "-map", "0",
        "-dn",
        "-vcodec", "libx264",
        "-vf", "scale=1920:-8",
        "-framerate", "1",
        "-r", "30",
        "-pix_fmt", "yuv420p",
        "-b:v", ($video_bitrate | tostring),
        "-b:a", "128000"
    ];


# --- Media type detection ---

def is_video_file($root):
    [$root.mediainfo.media.track[]["@type"]] | if (index("Video") != null) then true else false end;

def is_audio_file($root):
    [$root.mediainfo.media.track[]["@type"]] | if (index("Audio") != null) and is_video_file($root) != true then true else false end;

def is_image_file($root):
    [$root.mediainfo.media.track[]["@type"]] | if (index("Image") != null) and is_video_file($root) != true and is_audio_file($root) != true then true else false end;


# Main

. as $root |
# 1) Validate input structure
if ($root | [has("mediainfo"), has("clapshot_vars"), has("total_frames")] | all) then

    # 2) Detect media type and generate processing options
    if is_video_file($root) then {
        "media_type": "video",
        "transcode": video_decide_transcoding($root.mediainfo; $root.clapshot_vars.max_video_bitrate),
        "ffmpeg_thumbnail_options": ffmpeg_thumbnail_options(160; 90),
        "ffmpeg_thumbsheet_options": ffmpeg_thumbsheet_options($root.total_frames; 160;90; 10;10)
    } elif is_audio_file($root) then {
        "media_type": "audio",
        "transcode": audio_to_video_transcoding($root.clapshot_vars.max_video_bitrate),
        "ffmpeg_thumbnail_options": ffmpeg_thumbnail_options(160;90),
        "ffmpeg_thumbsheet_options": ffmpeg_thumbsheet_options($root.total_frames; 160;90; 10;10)
    } elif is_image_file($root) then {
        "media_type": "image",
        "transcode": image_to_video_transcoding($root.clapshot_vars.max_video_bitrate),
        "ffmpeg_thumbnail_options": ffmpeg_thumbnail_options(160;90),
        "ffmpeg_thumbsheet_options": ffmpeg_thumbsheet_options($root.total_frames; 160;90; 10;10)
    } else {
        "error": "Unsupported media type"
    }
    end
else
    {"error": "Missing field(s) in input structure"}
end
