# ADR-0005: Codec and HDR Prioritization, Audio Stream Handling, and Extraction Resilience

**Date**: 2026-10-16
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

Prior to this change, several media format resolution and extraction edge cases existed:
1. URL scheme verification in the interactive shell was case-sensitive (`!input.starts_with("http://") && !input.starts_with("https://")`), treating uppercase schemes (`HTTP://...`, `Https://...`) as unrecognized shell commands.
2. When downloading an audio-only format, `build_ytdlp_download_cmd` unconditionally added `+bestaudio/best` and `--merge-output-format mp4`, triggering an invalid attempt to merge two audio streams into an MP4 container.
3. Setting default quality to `8k` via configuration was supported by `is_valid_quality`, but `quality_opts` in interactive configuration prompts omitted `8k`. Furthermore, `find_format_by_quality` lacked an explicit `8k` branch and failed to match formats labeled `4320p`.
4. When a platform or URL returned zero downloadable formats, `resolve_media_to_download` fell through to `select_format`, which failed on the empty list and produced a misleading `WARNING: Download cancelled by user.` message.
5. In `parse_video_info_json`, streams where `w == h && w > 0` were unconditionally skipped under the assumption they were storyboard sprites. This discarded genuine 1:1 media (e.g., square Instagram/TikTok posts, VR 180/360 frames). Additionally, `resolve_resolution` checked horizontal widescreen thresholds first, misclassifying 1080x1080 square videos as 480p.
6. Video streams lacking integer `width` and `height` properties in the JSON response were skipped or collapsed into a single `"Default"` resolution, dropping alternate format tiers.
7. Candidate deduplication in `parse_video_info_json` sorted solely by `(rank, is_direct, is_exact, size_ranking)`. HDR metadata (`dynamic_range == "HDR"`) and video codec quality tiers were ignored, allowing higher-bitrate AVC/H.264 SDR streams to displace AV1/VP9 HDR streams of the identical resolution tier.

## Decision

1. **Case-Insensitive Shell URL Detection**:
   - Replaced prefix checks in `src/cli/shell.rs` with `is_valid_url(input)`, which parses URLs via the `url` crate where scheme comparisons adhere to RFC 3986 case-insensitivity.

2. **Differentiated Audio Command Generation**:
   - Added `MediaFormat::is_audio()` to reliably distinguish audio streams (`resolution == "Audio"` or `vcodec == "none" && acodec != "none"`).
   - In `resolve_media_to_download`, audio format IDs are tagged with an `audio:` prefix.
   - In `build_ytdlp_download_cmd`, audio streams omit `+bestaudio/best` and omit `--merge-output-format mp4`, downloading the stream directly in its native container.

3. **8K Format Resolution and Prompt Parity**:
   - Added `"8k"` to `quality_opts` in `src/cli/prompts.rs`.
   - Added an explicit `target == "8k"` check in `find_format_by_quality` matching both `"4320"` and `"8k"`.

4. **Explicit Empty Formats Error**:
   - In `resolve_media_to_download`, empty format lists immediately emit `ERROR: No downloadable formats found for this media stream.` and return `None` without prompting.
   - Added an early return `None` guard in `select_format` when `formats.is_empty()`.

5. **Square and VR/360 Video Preservation**:
   - Removed the `w == h` skip guard in `parse_video_info_json`, relying on existing format note filters to reject storyboard images.
   - Added a square video mapping branch in `resolve_resolution` mapping 1:1 dimensions (`w == h`) directly via `map_height_to_resolution(h)`.

6. **Fallback Dimension and Resolution Parsing**:
   - When integer `height` or `width` is missing, `parse_video_info_json` parses the `resolution` string (e.g., `1920x1080` or `720p`) or extracts height from format notes and IDs.
   - Unmeasured video streams fall back to format ID rather than collapsing all formats into `"Default"`.

7. **HDR and Modern Codec Prioritization**:
   - Introduced `score_vcodec` (AV1 [4] > VP9 [3] > HEVC [2] > AVC [1] > Other [0]) and `score_acodec` (Opus [3] > AAC [2] > MP3 [1] > Other [0]).
   - Video candidate sorting tuple updated to:
     `(rank, is_direct, is_hdr, codec_score, is_exact, size_ranking, format)`
   - Strictly preserves direct progressive HTTPS priority (`is_direct`) per `AGENTS.md` Invariant #6, while ensuring HDR and higher-efficiency codecs take precedence over legacy SDR streams.

## Consequences

### Positive
- Prevents invalid audio+audio merging and broken MP4 containers when downloading audio tracks.
- Accurate format selection for 8K, 1:1 square, and VR/360 content.
- Better visual fidelity by default: AV1/VP9 and HDR streams prioritized over bloated AVC SDR streams.
- Eliminates misleading cancellation messages on unextractable or empty streams.
- Shell accepts uppercase URL schemes cleanly.

### Negative
- None identified.
