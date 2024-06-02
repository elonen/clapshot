#!/bin/bash

#INPUTFILE="Apollo11-963-aae-excerpt.wav"
#INPUTFILE="Apollo11_countdown.mp3"
INPUTFILE="drunkards-special-short-mono.wav"
#INPUTFILE="sweep-tone.wav"
#INPUTFILE="11 - Gogo no Parade.ogg"

# Extract audio waveform into a png file
#ffmpeg -i input_audio.mp3 -filter_complex "[0:a]showwavespic=s=1920x1080:split_channels=1[full];[0]showwaves=s=1920x1080:mode=cline:colors=white[anim];[anim][full]overlay[out]" -map [out] -map 0:a output-joo.mp4

DURATION=$(ffprobe -i "$INPUTFILE" -show_entries format=duration -v quiet -of csv="p=0")
#ffmpeg -i input_audio.mp3 -filter_complex "color=c=red:s=1920x80[bar];[0:a]showwavespic=s=1920x1080:split_channels=1[full];[full][bar]overlay=t*10:0:shortest=1[intr];[intr]drawtext=text='Moicca! %{frame_num}': fontcolor=white@0.8: x=7: y=460[out]" -map [out] -map 0:a output_progr.mp4


#    [0:a] showwavespic=s=1920x720:split_channels=1:draw=scale [stillwave];\
#    color=c=white:s=2x720 [cursor]; \
#    [stillwave] loop=loop=-1:size=1:start=0 [stillwave_loop]; \
#    [cursor] loop=loop=-1:size=1:start=0 [cursor_loop]; \
#    [stillwave_loop]trim=duration=${DURATION},setpts=PTS-STARTPTS [stillwave_trim]; \
#    [cursor_loop]trim=duration=${DURATION},setpts=PTS-STARTPTS [cursor_trim]; \
#    [stillwave_trim][cursor_trim] overlay=(W*t)/(${DURATION}):180:shortest=1 [progress]; \
#    [progress] format=yuv420p [out] \


#ffmpeg -i "$INPUTFILE"  -filter_complex "\
#    [0:a] showwavespic=s=1920x720:split_channels=1:draw=scale:filter=peak, setpts=0 [stillwave];\
#    color=c=white:s=2x720 [cursor]; \
#    [stacked][cursor] overlay=(W*t)/(${DURATION}):180:shortest=1 [progress]; \
#    [0:a] showfreqs=mode=line:ascale=log:s=1920x180:rate=60 [freqwave]; \
#    [0:a] asetpts=PTS-STARTPTS, showwaves=mode=p2p:s=1920x180:rate=60, exposure=1.0, dilation [livewave]; \
#    [livewave][stillwave] vstack [stacked]; \
#    [progress][freqwave] vstack, format=yuv420p, fps=60 [out]; \
#    " -map "[out]" -map 0:a \
#    -c:v libx264 -c:a flac \
#    -y output_progr.mp4

ffmpeg -i "$INPUTFILE"  -filter_complex "\
    color=c=white:s=2x960 [cursor]; \
    [0:a] showwavespic=s=1920x960:split_channels=1:draw=scale:filter=peak:colors=#181818 [stillwave_peak];\
    [0:a] showwavespic=s=1920x960:split_channels=1:draw=scale:filter=average [stillwave_avg];\
    [stillwave_peak][stillwave_avg] overlay [stillwave];\
    \
    [stillwave] loop=loop=-1:size=1:start=0, fps=30, trim=duration=${DURATION} [stillwave_loop]; \
    [stillwave_loop][cursor] overlay=(W*t)/(${DURATION}):0:shortest=1 [progress]; \
    \
    [progress] format=yuv420p [out]; \
    " -map "[out]" -map 0:a \
    -c:v libx264 -c:a flac \
    -y output_progr.mp4


# Make a video from an audio file and a png file
#ffmpeg -i input_audio.mp3 -i output.png -filter_complex "[0:a]showwaves=s=1920x1080:mode=cline:colors=white[v];[1]scale=1920:1080[bg];[bg][v]overlay[out]" -map [out] -map 0:a output.mp4

#ffmpeg -i input_audio.mp3 -filter_complex "color=c=red:s=1920x10[bar];[0][bar]overlay=-w+(w/10)*t:H-h:shortest=1" -c:a copy output_progr.mp4

#ffmpeg -i input_audio.mp3 -filter_complex "color=c=red:s=1920x10[bar]" -c:a copy output_progr.mp4
