# OpenJOC v0.16.0 — Advanced Binaural Playback & Diagnostics

OpenJOC v0.15 focused on ETSI profile and carriage completeness. v0.16 focuses
on the playback experience: clear speaker/headphone choices, configurable
HRTF playback, and diagnostics that explain when OpenJOC yields to stock LAV.

## Highlights

- **Stereo (Speakers)** provides conventional two-channel speaker playback
  without HRTF processing.
- **Binaural (Headphones)** renders the OpenJOC scene through virtual speakers
  and a SOFA/HRTF stage into two-channel headphone PCM.
- **Built-in SADIE II D1** is the default 48 kHz KU100 HRTF.
- **Custom SOFA** is an explicit, validated local-file override.
- Virtual layouts are **7.1.4 (Recommended)** and **9.1.6 (Experimental)**.
- LAV Status distinguishes **OpenJOC**, **Stock decoder**, and **Stock decoder
  (OpenJOC fallback)**, with bounded failure details and AU context when known.
- OpenJOC Status meter updates remain smooth during rendering.

## Known limitations

- No head tracking or automatic headphone detection.
- Custom SOFA is user-selected local HRTF data, not automatic personalization.
- 9.1.6 is experimental; it is not claimed to be superior to 7.1.4.
- Final Binaural output is always two-channel; exact native Dolby/Apple
  binaural equivalence is not claimed.
- Flat-7.X to physical 5.1 automatic fold-down is unsupported.
- Status pages display only the first eight channels for outputs above eight
  channels.

The complete source, license, and build provenance is included with the
corresponding-source asset. The built-in SADIE II D1 resource retains the
University of York attribution and Apache License 2.0 notice.
