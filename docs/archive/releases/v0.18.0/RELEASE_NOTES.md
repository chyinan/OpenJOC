# OpenJOC v0.18.0 — built-in HRTF choices and Custom SOFA in WASM

OpenJOC v0.18.0 adds a second built-in HRTF and the WASM bridge needed by
OpenJOC Browser's local Custom SOFA importer.

## Highlights

- **A second built-in HRTF.** SADIE II D2 / KEMAR joins SADIE II D1 / KU100;
  D1 remains the default.
- **Built-in preset selection.** The CLI can select the binaural HRTF preset.
- **Custom SOFA in WASM.** A bounded allocator and decoder constructor accept
  supported local SimpleFreeFieldHRIR SOFA files for the browser importer.
- **LAV binaural settings.** The settings page adds the D1/D2 selector and a
  Custom SOFA option.
- **C ABI package metadata.** Package metadata now identifies C ABI 1.6, and
  the built-in HRTF documentation and provenance are refreshed.
- **Bounded SOFA parsing.** WASM parsing enforces explicit limits for file
  size, measurements, FIR length, delays, and coefficients. HRTF preset matching
  is exhaustive across the API and WASM renderer.

## Release contract

- Release version: `0.18.0`.
- Windows LAV package: `openjoc-lav-0.18.0-windows-x64.zip`.
- The LAV package is built from commit
  `8f32aad51aea24602f2175316a241568be5fc4a1`.

## Boundaries

The built-in HRTFs are generic and are not individualized. Custom SOFA support
is limited to the documented SimpleFreeFieldHRIR subset and the explicit parser
limits above.

# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
