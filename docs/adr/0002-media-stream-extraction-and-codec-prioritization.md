# ADR-0002: Media Stream Extraction, Codec Prioritization, and Format Selection

**Date**: 2026-10-16  
**Status**: Accepted  

## Context

During early development, stream extraction and format resolution presented several defects:
1. **Unprobed Codec Failure**: Non-YouTube and generic websites often returned `vcodec: null` and `acodec: null`. DWDR discarded these streams, falsely reporting zero extracted formats.
2. **False Cancellation Error**: Single available streams or fallback streams opened empty interactive prompts, failing with `WARNING: Download cancelled by user.` when the user never cancelled.
3. **Audio-Only Merging Bug**: Downloading dedicated audio tracks passed `+bestaudio/best` and `--merge-output-format mp4`, attempting an invalid merge of two audio streams into an MP4 container.
4. **Square and VR Media Dropped**: Media with `width == height` was discarded under the assumption it was storyboard thumbnails, dropping genuine 1:1 Instagram/TikTok and VR 180/360 videos.
5. **Inefficient Codec Preference**: High-bitrate legacy AVC/H.264 SDR streams displaced modern AV1/VP9 and HDR streams of identical resolution tiers.

## Decision

1. **Heuristic Media Recognition**:
   - Detect streams by file extension (`mp4`, `webm`, `mkv`, `mp3`, `m4a`) and spatial dimensions when codec metadata is `null` or missing.
   - Synthesize a default `"best"` format fallback if no format array is returned.

2. **Single-Stream Auto-Selection**:
   - When only 1 format exists, start downloading immediately without presenting an interactive selection prompt.
   - When 0 formats exist, fallback to `"best"`.
   - Emit cancellation warnings only when the user explicitly aborts a prompt.

3. **Dedicated Audio Stream Handling**:
   - Use `MediaFormat::is_audio()` to identify audio tracks.
   - Tag audio format IDs with `audio:` prefix in the downloader.
   - Omit `+bestaudio/best` and `--merge-output-format mp4` when downloading audio, saving in the native container.

4. **Aspect Ratio and Resolution Mapping**:
   - Retain 1:1 aspect ratio media and map square dimensions using vertical height.
   - Extract dimensions from format notes or IDs when integer width/height values are missing.

5. **Codec and HDR Prioritization**:
   - Rank video codecs (AV1 > VP9 > HEVC > AVC) and audio codecs (Opus > AAC > MP3).
   - Prefer HDR streams over SDR at the same resolution.
   - Preserve direct progressive HTTPS streams over adaptive HLS manifests to prevent CDN 403 rate limits.

## Consequences

- Reliable extraction and downloading across both YouTube and non-YouTube platforms.
- Native, uncorrupted audio downloads.
- Automatic selection without redundant prompts when only one format tier is available.
