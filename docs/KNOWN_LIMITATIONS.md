# Known limitations

The canonical current limitations are maintained in the [OpenJOC documentation site](site/compatibility/known-limitations.md).

The fixed DirectShow output contract covers exactly Stereo, Binaural, 5.1,
7.1, 5.1.2, 5.1.4, 7.1.2, and 7.1.4. Automatic downstream layout discovery is
`AUTO_NOT_RELIABLE`. Physical multichannel hardware is not verified.

The explicit Rust/C listener-orientation API accepts host-supplied 3DoF poses
for virtual-speaker binaural rendering; it does not read sensors or control a
device. Preparation p95 exceeded its 4 ms target in the measured host probe,
and no sensor-to-sound latency or real-earphone claim is established. See the
canonical limitations page for the benchmark scope and exact bounds.

This repository path is retained as a short compatibility pointer for existing links.
