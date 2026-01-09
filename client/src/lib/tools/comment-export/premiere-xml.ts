import type { CommentExporter, ExportContext } from './types';

/**
 * Converts timecode "HH:MM:SS.mmm" to frame number.
 */
function timecodeToFrames(tc: string, fps: number): number {
    if (!tc) return 0;

    // Parse HH:MM:SS.mmm format
    const match = tc.match(/^(\d{2}):(\d{2}):(\d{2})\.(\d{3})$/);
    if (match) {
        const [, hh, mm, ss, ms] = match.map(Number);
        const totalSeconds = hh * 3600 + mm * 60 + ss + ms / 1000;
        return Math.round(totalSeconds * fps);
    }

    // Try HH:MM:SS:FF format
    const edlMatch = tc.match(/^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/);
    if (edlMatch) {
        const [, hh, mm, ss, ff] = edlMatch.map(Number);
        return hh * 3600 * fps + mm * 60 * fps + ss * fps + ff;
    }

    return 0;
}

function escapeXml(str: string): string {
    return str
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&apos;');
}

function generateMarkerXml(comment: string, name: string, frameIn: number): string {
    return `            <marker>
              <comment>${escapeXml(comment)}</comment>
              <name>${escapeXml(name)}</name>
              <in>${frameIn}</in>
              <out>-1</out>
              <pproColor>4294741314</pproColor>
            </marker>`;
}

export const premiereXmlExporter: CommentExporter = {
    id: 'premiere-xml',
    name: 'Adobe Premiere Pro XML',
    extension: '.xml',
    options: [
        {
            id: 'sequenceName',
            label: 'Sequence name',
            type: 'string',
            default: '',
        },
    ],
    export(ctx: ExportContext, opts: Record<string, unknown>): string {
        const fps = Math.round(ctx.fps || 25);
        const seqName = (opts.sequenceName as string) || `Marker - ${ctx.title}`;
        const now = new Date();
        const dateStr = now.toLocaleString('fi-FI').replace(',', '').replace(/\./g, '.').replace('klo ', '');

        // Calculate total duration in frames
        const totalDuration = ctx.duration ? Math.round(ctx.duration * fps) : 90000;

        // Generate markers for sequence level
        const sequenceMarkers = ctx.comments.map(c => {
            const frameIn = timecodeToFrames(c.timecode, fps);
            let text = c.text;
            if (c.hasDrawing && !text.includes('[has drawing]')) {
                text += ' [has drawing]';
            }
            return generateMarkerXml(text, c.username || '', frameIn);
        }).join('\n');

        // Generate markers for clip level (duplicate for compatibility)
        const clipMarkers = sequenceMarkers;

        const uuid = crypto.randomUUID().replace(/-/g, '');

        return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xmeml>
<xmeml version="4">
  <sequence id="sequence" TL.SQAudioVisibleBase="0" TL.SQVideoVisibleBase="0" TL.SQVisibleBaseTime="0" TL.SQAVDividerPosition="0.5" TL.SQHideShyTracks="0" TL.SQHeaderWidth="292" Monitor.ProgramZoomOut="0" Monitor.ProgramZoomIn="0" TL.SQTimePerPixel="0.19999999999999998" MZ.EditLine="0" MZ.Sequence.PreviewFrameSizeHeight="1080" MZ.Sequence.PreviewFrameSizeWidth="1920" MZ.Sequence.AudioTimeDisplayFormat="200" MZ.Sequence.PreviewRenderingClassID="1061109567" MZ.Sequence.PreviewRenderingPresetCodec="1634755439" MZ.Sequence.PreviewRenderingPresetPath="EncoderPresets/SequencePreview/795454d9-d3c2-429d-9474-923ab13b7018/QuickTime.epr" MZ.Sequence.PreviewUseMaxRenderQuality="false" MZ.Sequence.PreviewUseMaxBitDepth="false" MZ.Sequence.EditingModeGUID="795454d9-d3c2-429d-9474-923ab13b7018" MZ.Sequence.VideoTimeDisplayFormat="101" MZ.WorkOutPoint="${totalDuration * 208000}" MZ.WorkInPoint="0" explodedTracks="true">
    <uuid>${uuid}</uuid>
    <duration>${totalDuration}</duration>
    <rate>
      <timebase>${fps}</timebase>
      <ntsc>FALSE</ntsc>
    </rate>
    <name>${escapeXml(seqName)} (${dateStr})</name>
    <media>
      <video>
        <format>
          <samplecharacteristics>
            <rate>
              <timebase>${fps}</timebase>
              <ntsc>FALSE</ntsc>
            </rate>
            <codec>
              <name>Apple ProRes 422</name>
              <appspecificdata>
                <appname>Final Cut Pro</appname>
                <appmanufacturer>Apple Inc.</appmanufacturer>
                <appversion>7.0</appversion>
                <data>
                  <qtcodec>
                    <codecname>Apple ProRes 422</codecname>
                    <codectypename>Apple ProRes 422</codectypename>
                    <codecname>Apple ProRes 422</codecname>
                    <codectypecode>apcn</codectypecode>
                    <codecvendorcode>appl</codecvendorcode>
                    <spatialquality>1024</spatialquality>
                    <temporalquality>0</temporalquality>
                    <keyframerate>0</keyframerate>
                    <datarate>0</datarate>
                  </qtcodec>
                </data>
              </appspecificdata>
            </codec>
            <width>1920</width>
            <height>1080</height>
            <anamorphic>FALSE</anamorphic>
            <pixelaspectratio>square</pixelaspectratio>
            <fielddominance>none</fielddominance>
            <colordepth>24</colordepth>
          </samplecharacteristics>
        </format>
        <track TL.SQTrackShy="0" TL.SQTrackExpandedHeight="25" TL.SQTrackExpanded="0" MZ.TrackTargeted="0">
          <enabled>TRUE</enabled>
          <locked>FALSE</locked>
          <generatoritem id="clipitem-1">
            <name>Marker Color Matte (${dateStr})</name>
            <enabled>TRUE</enabled>
            <duration>${totalDuration}</duration>
            <rate>
              <timebase>${fps}</timebase>
              <ntsc>FALSE</ntsc>
            </rate>
            <start>0</start>
            <end>${totalDuration}</end>
            <in>0</in>
            <out>${totalDuration}</out>
            <alphatype>none</alphatype>
            <effect>
              <name>Color</name>
              <effectid>Color</effectid>
              <effectcategory>Matte</effectcategory>
              <effecttype>generator</effecttype>
              <mediatype>video</mediatype>
              <parameter authoringApp="PremierePro">
                <parameterid>fillcolor</parameterid>
                <name>Color</name>
                <value>
                  <alpha>0</alpha>
                  <red>0</red>
                  <green>0</green>
                  <blue>0</blue>
                </value>
              </parameter>
            </effect>
            <filter>
              <effect>
                <name>Opacity</name>
                <effectid>opacity</effectid>
                <effectcategory>motion</effectcategory>
                <effecttype>motion</effecttype>
                <mediatype>video</mediatype>
                <pproBypass>false</pproBypass>
                <parameter authoringApp="PremierePro">
                  <parameterid>opacity</parameterid>
                  <name>opacity</name>
                  <valuemin>0</valuemin>
                  <valuemax>100</valuemax>
                  <value>0</value>
                </parameter>
              </effect>
            </filter>
${clipMarkers}
          </generatoritem>
        </track>
      </video>
      <audio>
        <numOutputChannels>2</numOutputChannels>
        <format>
          <samplecharacteristics>
            <depth>16</depth>
            <samplerate>48000</samplerate>
          </samplecharacteristics>
        </format>
        <outputs>
          <groups>
            <index>1</index>
            <numchannels>1</numchannels>
            <downmix>0</downmix>
            <channel>
              <index>1</index>
            </channel>
          </groups>
          <groups>
            <index>2</index>
            <numchannels>1</numchannels>
            <downmix>0</downmix>
            <channel>
              <index>2</index>
            </channel>
          </groups>
        </outputs>
      </audio>
    </media>
    <timecode>
      <rate>
        <timebase>${fps}</timebase>
        <ntsc>FALSE</ntsc>
      </rate>
      <string>01:00:00:00</string>
      <frame>90000</frame>
      <displayformat>NDF</displayformat>
    </timecode>
    <labels>
      <label2>Iris</label2>
    </labels>
    <logginginfo>
      <description/>
      <scene/>
      <shottake/>
      <lognote/>
      <good/>
      <originalvideofilename/>
      <originalaudiofilename/>
    </logginginfo>
${sequenceMarkers.split('\n').map(line => '    ' + line.trim()).join('\n')}
  </sequence>
</xmeml>
`;
    }
};
