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
