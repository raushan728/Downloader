# ADR-0003: Unprobed Codec Support and Single-Stream Auto-Selection

**Date**: 2026-10-15
**Status**: accepted
**Deciders**: DWDR Core Team

## Context

When extracting media info (`yt-dlp -J`) from generic or non-YouTube platforms (e.g. niche streaming or direct video hosts), extractors often return format dictionaries without pre-probed codec strings (`vcodec: null`, `acodec: null`). Previously, `parse_video_info_json` strictly filtered out any stream where `vcodec == "none" && acodec == "none"`, discarding valid playable streams and producing `Extracted 0 formats.`.

Furthermore, when zero formats remained or a single format existed, `resolve_media_to_download` routed through interactive selection (`inquire::Select`), which immediately failed with an error on empty choices and printed `WARNING: Download cancelled by user.` even though the user never pressed Ctrl+C or cancelled the prompt.

## Decision

1. **Heuristic Media Recognition**: Recognize video and audio candidates by file extensions (`mp4`, `webm`, `mkv`, `mp3`, `m4a`, etc.) and dimensions (`height`, `width`) when `vcodec` and `acodec` are `null` or unprobed.
2. **Flexible Resolution Fallback**: When pixel dimensions cannot be mapped into standard 16:9 or portrait resolutions, fall back to `format_note` (e.g., "Source", "720p") or the raw height value before falling back to `"Default"`.
3. **Stream Synthesis Fallback**: When a platform returns unpartitioned top-level media without a `formats` array, synthesize a default `MediaFormat` (`format_id: "best"`, `resolution: "Default"`).
4. **Auto-Selection for Single & Fallback Streams**: When 0 formats are enumerated, auto-select `"best"` (`bestvideo+bestaudio/best`). When exactly 1 format exists, auto-start that format without presenting a redundant interactive selection prompt.
5. **Accurate User Cancellation**: Only display cancellation warnings when the user deliberately aborts an active interactive selection prompt.

## Alternatives Considered

### Alternative 1: Force yt-dlp to Probe All Codecs via Network Requests
- **Pros**: Guarantees populated `vcodec` and `acodec` fields in JSON.
- **Cons**: Significantly slows down initial metadata extraction by requiring multiple HTTP HEAD/range requests before downloading.
- **Why not**: Slower user experience and increases chances of triggering anti-scraping rate limits.

### Alternative 2: Abort Immediately on Unprobed Codecs
- **Pros**: Ensures only strictly validated formats are listed.
- **Cons**: Completely breaks downloads on thousands of non-YouTube video platforms supported by yt-dlp.
- **Why not**: Violates DWDR's goal as a universal media downloader.

## Consequences

### Positive
- Universal compatibility across non-YouTube and generic media websites.
- Zero unnecessary prompt interruptions when only one format tier is available.
- Eliminates misleading "cancelled by user" error messages.

### Negative
- Videos with non-standard resolutions will display their note or height instead of a normalized standard label.

### Risks
- If a platform serves a format with no codec metadata and an unrecognized extension, it will fall back to `"best"` for yt-dlp to resolve at download time.
