# OpenJOC v0.17.0 — JOC Stream live inspection

OpenJOC v0.17.0 is a feature release, not a v0.16.1 patch release. Its primary
feature is the LAV **JOC Stream** live inspector.

## Highlights

- **LAV JOC Stream live inspector.** The Windows side-by-side LAV filter now
  exposes a read-only property page backed by the current in-band decoder
  session. It shows JOC presence, profile, programme layout, reconstruction
  carriers, topology, dependent IDs, block partition, LFE/JOC ownership,
  validation results, EMDF payloads, dynamic-scene observation, malformed and
  access-unit counters, timestamps, and coverage. `Copy JSON` exports a
  sanitized versioned `live_decode_snapshot` document.
- **Standards-aware offline Inspector.** `openjoc inspect` reports bounded raw
  E-AC-3 and seekable MP4/fragmented-container observations using versioned
  JSON schema 1. It separates ETSI Strict from Deployed Compatibility results,
  keeps JOC profile/carrier semantics, and reports EMDF/OAMD/container
  observations without inferring JOC from filenames or external codec labels.
- **Live versus full-stream semantics.** LAV values are explicitly
  observed-so-far and belong to the current decode epoch. Seek, flush, or
  discontinuity starts a new epoch; EOS after a seek is partial unless
  continuous coverage from the beginning is proven. The offline Inspector
  remains the full-stream forensic tool.
- **Programme layout versus reconstruction carriers.** Inspector, C ABI, and
  LAV output now expose the actual programme layout separately from the JOC
  reconstruction-carrier plane. This distinction covers the supported profile
  pairs, including Flat-7.X.
- **C ABI 1.5 live surface.** The stream decoder exposes a bounded
  `openjoc_live_inspection_snapshot` plus caller-requested sanitized live JSON,
  with versioned structure sizing and no JSON serialization on the decode
  observer update path.
- **WASM integration.** The WASM bridge adds Chromium Stereo support,
  timestamped CMAF packet input, OpenJOC binaural rendering, and status and
  performance reporting. The binaural path uses the built-in SADIE II D1 HRTF by
  default.
- **Flat-7.X Speaker 2.0 closure.** Stereo routing admits distinct `Lrs` and
  `Rrs` rear-surround inputs under the validated Flat-7.X matrix, with one-hot
  identity/mirror coverage and the existing same-side and overflow-scaling
  rules. This does not add automatic Flat-7.X-to-physical-5.1 fold-down.
- **Compatibility and regression fixes.** The release keeps the LAV admission
  prefix across live DirectShow chunks, avoids live-inspector receive-lock
  starvation, makes CMAF fixture generation deterministic, and isolates host
  media tools from the FFmpeg SDK environment in CI/release jobs.

## Release contract

- LAV integration pin:
  `147c24fe1489ded5473c2a09987430ca1070a412`
- Windows LAV binary:
  `openjoc-lav-0.17.0-windows-x64.zip`
- Windows LAV corresponding source:
  `openjoc-lav-0.17.0-corresponding-source.zip`
- Supported core asset families remain the v0.16.0 families: macOS arm64,
  Windows x86_64, and GNU/Linux x86_64 core archives, plus the mpv, SDK,
  FFmpeg, and GStreamer package families for those targets, each with the
  release checksum manifest.

## Boundaries

The LAV page is a playback diagnostic view, not a rescan or a second decode.
Its observed-so-far values must not be read as full-stream proof. Existing
experimental ABI, endpoint, physical-hardware, and native-renderer-equivalence
boundaries remain unchanged unless stated above.

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
