PRAGMA foreign_keys=OFF;

-- Step 1: Create the media_types table
CREATE TABLE media_types (
    id VARCHAR(32) NOT NULL PRIMARY KEY,
    precedence INTEGER NOT NULL,  -- Used to determine precedence of media types when multiple types are detected. Lower value means higher precedence.
    jq_script TEXT NOT NULL,   -- jq script for media type detection
    ffmpeg_options TEXT NOT NULL  -- FFMPEG options for converting media to video
);

-- Step 2: Insert initial media types
INSERT INTO media_types (id, precedence, jq_script, ffmpeg_options) VALUES
(   'video', 0,

    -- jq script for video: Check if any track is of type "Video"
    '[.media.track[]["@type"]] | if (index("Video") != null) then "Video" else empty end',

    -- FFMPEG script for video: Scale to max 1920x, AAC audio, 128kbps, 2 channels
    '
        -map 0 -dn
        -vcodec libx264
        -vf scale=1920:-8
        -preset faster
        -acodec aac
        -ac 2
        -strict experimental
        -b:v %%VIDEO_BITRATE%%
        -b:a 128000
    '
),
(   'audio', 1,

    -- jq script for audio: Check if any track is of type "Audio" and not of type "Video"
    '[.media.track[]["@type"]] | if (index("Audio") != null) and (index("Video") == null) then "Audio" else empty end',

    -- FFMPEG script for audio: Show audio waveform, frequency spectrum, and live waveform, 1920x1080 total, 60fps
    '
        -dn -r 60 -filter_complex
        "   color=c=white:s=2x720 [cursor];
            [0:a] showwavespic=s=1920x720:split_channels=1:draw=full, fps=60 [stillwave];
            [0:a] showfreqs=mode=line:ascale=log:s=1920x180 [freqwave];
            [0:a] showwaves=size=1920x180:mode=p2p [livewave];
            [stillwave][cursor] overlay=(W*t)/%%DURATION%%:0:shortest=1 [progress];
            [livewave][progress] vstack[stacked];
            [stacked][freqwave] vstack [out];"
        -map "[out]" -map 0:a
        -strict experimental
        -vcodec libx264
        -b:v %%VIDEO_BITRATE%%
        -acodec flac
    '
),
(   'image', 2,

    -- jq script for image: Check if any track is of type "Image" and not of type "Video" or "Audio"
    '[.media.track[]["@type"]] | if (index("Image") != null) and (index("Video") == null) and (index("Audio") == null) then "Image" else empty end',

    -- FFMPEG script for image: Scale to max 2048x2048, 1 second duration
    '
        -map 0
        -dn
        -vcodec libx264
        -vf scale=1920:-8
        -framerate 1
        -r 30
        -pix_fmt yuv420p
        -b:v %%VIDEO_BITRATE%%
        -b:a 128000
    '
);

-- Step 3: Add media_type column
ALTER TABLE videos ADD COLUMN media_type VARCHAR(32) DEFAULT NULL REFERENCES media_types (id) ON DELETE RESTRICT ON UPDATE CASCADE;
UPDATE videos SET media_type = 'video';

-- Step 4: Rename videos table
ALTER TABLE videos RENAME TO media_files;

-- Step 5: Rename video_id column to media_file_id in all tables
ALTER TABLE comments RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_comments_video_id;
CREATE INDEX IF NOT EXISTS ix_comments_media_file_id ON comments (media_file_id);

ALTER TABLE messages RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_messages_video_id;
CREATE INDEX IF NOT EXISTS ix_messages_media_file_id ON messages (media_file_id);

PRAGMA foreign_keys=ON;
