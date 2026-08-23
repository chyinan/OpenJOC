# OpenJOC v0.10.0 — Windows DirectShow / LAV Filters Integration

OpenJOC v0.10.0 adds an optional OpenJOC-enabled LAV Audio Decoder for the
Windows DirectShow ecosystem.

- PotPlayer validation covers raw and MP4 E-AC-3 JOC, positive JOC admission,
  ordinary E-AC-3 isolation, passthrough precedence, seek, EOS, reopen,
  stop/reopen, side-by-side installation, uninstall, and stock LAV rollback.
- The current DirectShow output is 48 kHz stereo float PCM.
- Public source: [LAVFilters-OpenJOC](https://github.com/chyinan/LAVFilters-OpenJOC),
  branch `openjoc-main`, tag
  [`openjoc-0.10.0`](https://github.com/chyinan/LAVFilters-OpenJOC/releases/tag/openjoc-0.10.0).
- The corresponding-source ZIP is attached alongside the frozen LAV binary
  ZIP and contains the recursive source and license closure.

OpenJOC core/CLI/SDK/C ABI code remains Apache-2.0. The downstream LAV
integration follows the applicable GPL-compatible upstream terms, with the
combined LAV distribution classified as GPL-3.0-only. This project is not
endorsed by LAV Filters, FFmpeg, PotPlayer, Dolby, Microsoft, or SADIE.
