# Third-party data notices

## SADIE II Database — D1 HRIRs

OpenJOC bundles a derived CDF-1 representation of the SADIE II Database D1
KU100 HRIR set (48 kHz, 256 taps, 8,802 measured directions).

- Publisher: The Audio Lab, Department of Electronic Engineering, University
  of York, United Kingdom.
- Official dataset page: <https://www.york.ac.uk/sadie-project/database.html>
- Official current distribution: the D1 HRIR SOFA link published from the
  SADIE II page, <https://zenodo.org/records/12092466>
- License: Apache License, Version 2.0, as stated on the official University
  of York SADIE II page; license text:
  <https://www.apache.org/licenses/LICENSE-2.0>.
- Required attribution: measurements are Copyright University of York; retain
  the SADIE II attribution and identify the source when the data is used in
  original or modified form.
- Academic citation: Cal Armstrong, Lewis Thresh, and Gavin Kearney,
  “A Perceptual Evaluation of Individual and Non-Individual HRTFs: A Case
  Study of the SADIE II Database,” DOI
  <https://doi.org/10.3390/app8112029>.

The bundled data is not OpenJOC source code and is not relabeled Apache-2.0
OpenJOC code. Its upstream source, conversion, and generated hashes are
recorded in `docs/SPATIAL_PORTABILITY.md` and the repository provenance notes.

## GStreamer integration dependencies

The optional `gst-plugin-openjoc` build uses the official gstreamer-rs crates
(`gstreamer`, `gstreamer-base`, `gstreamer-audio`, and `gstreamer-app`). Those
Rust bindings are distributed under MIT OR Apache-2.0. The native GStreamer
runtime and plugin modules retain their upstream licenses; this repository does
not vendor or relabel the GStreamer SDK. See the integration build notes in
`docs/integration/GSTREAMER.md` before distributing a combined runtime.

## OpenJOC Player Bundle components

The 0.8.0 `openjoc-mpv` packages are project-provided custom builds, not
official mpv or FFmpeg releases. The exact pinned FFmpeg and mpv source
commits, exported patch SHA-256 values, configure flags, and per-package
runtime inventory are recorded in
`packaging/player/PLAYER_PACKAGE_MANIFEST.json`, `BUILD_INFO.json`, and
`DEPENDENCIES.json` inside each package.

- The bundled OpenJOC libraries and CLI components remain Apache-2.0.
- The selected FFmpeg recipe uses shared libraries, `--enable-version3`, and
  no `--enable-gpl`; the package records the resulting component-specific
  license evidence and source files.
- mpv is distributed under its upstream GPL-2.0-or-later terms, with its
  license/source evidence copied into each package.
- Other bundled runtime libraries retain their own upstream licenses. The
  package verifier rejects unresolved component mappings, and each archive
  carries `THIRD_PARTY_NOTICES.txt`, `DEPENDENCIES.json`, and the exact inner
  `SHA256SUMS` manifest.

This notice records engineering provenance and required attribution surfaces;
it is not a blanket legal conclusion for every possible redistribution mode.
