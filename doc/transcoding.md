# Scriptable Transcoding and Thumbnailing

Clapshot supports customizable transcoding and thumbnailing through external scripts. This allows for hardware acceleration, custom encoders, and specialized processing workflows.

## Overview

Clapshot includes default scripts for media processing that can be customized or replaced:

- **clapshot-transcode-decision**: Determines whether a media file needs transcoding
- **clapshot-transcode**: Converts media files to web-compatible formats
- **clapshot-thumbnail**: Generates poster thumbnails and thumbnail sheets

These scripts are invoked by the Clapshot Server during media ingestion and can be customized for specific hardware or encoding requirements.

## Default Scripts

### Transcoding Decision Script (`clapshot-transcode-decision`)

The transcoding decision script evaluates whether a media file needs transcoding before the actual transcoding process begins. This allows you to:

- Customize which codecs and containers are considered web-compatible
- Implement site-specific policies for bitrate and quality requirements
- Skip transcoding for already-compatible formats to save processing time

The script receives media information via environment variables (media type, codec, container, bitrate) and outputs a JSON decision indicating whether transcoding is needed and at what bitrate.

By default, the script skips transcoding for browser-compatible formats (H.264, HEVC, VP8, VP9, AV1 in MP4/MKV/WebM containers) when the bitrate is acceptable.

### Transcoding Script (`clapshot-transcode`)

The default transcoding script handles three media types:

- **Video**: Converts to H.264 MP4 with configurable bitrate, max 1080p resolution
- **Audio**: Generates waveform visualization videos for audio-only files
- **Image**: Creates short video loops from still images

The script automatically selects appropriate encoding parameters based on the media type and target bitrate specified by the server.

### Thumbnailing Script (`clapshot-thumbnail`)

The thumbnailing script creates:

- **Poster thumbnails**: Single frame thumbnails
- **Thumbnail sheets**: Grid of frames distributed throughout the video for scrub preview

Thumbnails are generated only for appropriate media types (no thumbnails for audio files).

## Configuration

### Script Selection

On low level, the custom scripts are set via command-line arguments to Clapshot Server:

```bash
clapshot-server --transcode-decision-script /path/to/custom-decision \
                --transcode-script /path/to/custom-transcode \
                --thumbnail-script /path/to/custom-thumbnail
```

However,
- on Debian deployments, you'll want to edit `/etc/clapshot-server.conf` instead, and
- on Docker ones, use the `CLAPSHOT_SERVER__` environment variables to set the corresponding variables.

These will both set those command line options under the hood. See [Sysadmin Guide](sysadmin-guide.md) guide for more details.


### Hardware Acceleration

The default transcoding script supports hardware FFMPEG acceleration. You can enable one through a configuration file:

```bash
# Edit /etc/clapshot-transcode.conf
VIDEO_CODEC=h264_nvenc    # NVIDIA GPU encoding
HWACCEL=nvenc
VIDEO_FILTER_SCALE=scale_cuda
```

Supported acceleration options:

- **Intel Quick Sync Video (QSV)**: `h264_qsv`
- **NVIDIA NVENC**: `h264_nvenc`
- **VA-API (Intel/AMD)**: `h264_vaapi`
- **Apple VideoToolbox**: `h264_videotoolbox`


## Script Interface

### Environment Variables

All scripts receive input from Server through environment variables:

#### Transcoding Decision Variables
- `CLAPSHOT_MEDIA_TYPE`: Media type (`video`, `audio`, or `image`)
- `CLAPSHOT_ORIG_CODEC`: Original codec (e.g., `h264`, `vp9`)
- `CLAPSHOT_CONTAINER`: Container extension (e.g., `mp4`, `mkv`)
- `CLAPSHOT_BITRATE`: Original bitrate in bits per second
- `CLAPSHOT_TARGET_BITRATE`: Server's target max bitrate
- `CLAPSHOT_DURATION`: Duration in seconds
- `CLAPSHOT_INPUT_FILE`: Path to source file
- `CLAPSHOT_METADATA_JSON`: Full mediainfo JSON output (for advanced decisions)

The decision script must output JSON to stdout:
```json
{"transcode": true, "reason": "codec not supported", "bitrate": 2500000}
{"transcode": false}
```

#### Common Variables (Transcoding & Thumbnailing)
- `CLAPSHOT_INPUT_FILE`: Path to source media file
- `CLAPSHOT_OUTPUT_DIR`: Directory for output files
- `CLAPSHOT_MEDIA_TYPE`: Media type (`video`, `audio`, or `image`)
- `CLAPSHOT_USER_ID`: User identifier for logging
- `CLAPSHOT_MEDIA_ID`: Media file identifier
- `CLAPSHOT_DURATION`: Media duration in seconds

#### Transcoding-Specific
- `CLAPSHOT_OUTPUT_PREFIX`: Filename prefix for output
- `CLAPSHOT_TARGET_BITRATE`: Target bitrate in bits per second
- `CLAPSHOT_PROGRESS_PIPE`: Named pipe for progress reporting (optional)

#### Thumbnailing-Specific
- `CLAPSHOT_THUMB_SIZE`: Thumbnail dimensions (e.g., `160x90`)
- `CLAPSHOT_SHEET_DIMS`: Sheet grid dimensions (e.g., `10x10`)

See the default scripts on how these are used with FFMPEG.

### Progress Reporting

Transcoding scripts can (should) report progress by writing to `CLAPSHOT_PROGRESS_PIPE`:

```bash
# FFmpeg-style progress format
frame=584
fps=52.40
speed=2.1x
progress=continue

# Final completion
progress=end
```

This will cause Client to show a progress bar to the user.

### Exit Codes

Scripts should exit with status 0 for success, non-zero for failure. The server captures stdout/stderr for logging and troubleshooting.


### Debugging

Script execution outputs are written to dedicated log files:
- **Transcoding logs**: `<media-dir>/<media-id>/transcode.log`
- **Thumbnailing logs**: `<media-dir>/<media-id>/thumbs/thumbnail.log`
- **Server logs**: Check for log file locations when processing fails
- Use `--log-level debug` for detailed execution information
- Test scripts manually with environment variables set

Log files contain timestamps, command details, exit status, and complete stdout/stderr output from the scripts.

## Related Documentation

- [Architecture Overview](architecture-overview.md): Overall system design
- [Sysadmin Guide](sysadmin-guide.md): Server configuration and deployment
- [Development Setup](development-setup.md): Building and testing custom scripts