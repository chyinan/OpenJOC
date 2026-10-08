# OpenJOC v0.19.0 — Custom SOFA, listener orientation, and playback fixes

OpenJOC v0.19.0 expands Custom SOFA and experimental listener orientation,
improves binaural processing efficiency, and fixes decoder and playback edge cases.
This summary covers the [published v0.19.0 release](https://github.com/chyinan/OpenJOC/releases/tag/v0.19.0);
see [CHANGELOG.md](CHANGELOG.md) for the full release chronology.

## Highlights

- **Experimental listener orientation.** Host-supplied 3DoF orientation is
  available through the Rust and C APIs (C ABI 1.7), with canonical 22.2
  virtual-speaker directions.
- **Expanded Custom SOFA.** Native and WASM builds share bounded local
  NetCDF-4/HDF5 SimpleFreeFieldHRIR loading and setup-time HRIR sample-rate
  conversion, with fixes for conversion gain, delay, and supported dimensions.
- **Binaural efficiency.** QMF phase-row indexing and static direct-FIR
  processing are optimized without changing arithmetic order or PCM bits on
  the covered regression corpus.
- **Bounded CLI input.** Render input is read in access units after complete
  preflight, removing the 512 MiB programme cap while preserving sample order
  and drain behavior.
- **LAV Output gain.** The matching Windows LAV package adds persistent
  -20.0 to +20.0 dB gain with a bit-exact 0 dB bypass, and fixes preroll format
  transitions and settings persistence.
- **Decoder and playback fixes.** Corrections cover E-AC-3 coupling and SPX,
  selected CLI HRTFs, multi-AU PCM pulls, timestamp anchoring, WAV/ADM output,
  and portable package runtime paths and metadata.

## Release contract

- Release version: `0.19.0`, published on 2026-10-07.
- Source tag: [`v0.19.0`](https://github.com/chyinan/OpenJOC/tree/v0.19.0),
  commit `291900ce33ea349c4b855655cdb282819e94eabc`.
- Minimum supported Rust version: `1.89`.
- Windows LAV package: `openjoc-lav-0.19.0-windows-x64.zip`.
- The LAV package is built from commit
  `d13b7cac86c5750b5d4181569d98f44ae9a1607c`.

## Boundaries

Decoder correctness fixes can change affected PCM; v0.19.0 is not claimed to
produce identical PCM to v0.18.0 for every input. The bit-exact performance
changes are separately covered by regression gates and public synthetic Linux
measurements, which do not establish Windows/LAV speed or physical-device latency.

Listener orientation and virtual 9.1.6 remain experimental. Orientation is
host-supplied, with no sensor integration or automatic headphone detection;
binaural output remains two-channel. Physical multichannel hardware remains
unverified, and exact native Dolby/Apple binaural equivalence is not claimed.
Nonzero LAV gain has no clipping protection.

For current support and limits, see [Capabilities](docs/CAPABILITIES.md) and
[Known limitations](docs/KNOWN_LIMITATIONS.md).

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
