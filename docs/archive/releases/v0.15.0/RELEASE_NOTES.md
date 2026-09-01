# OpenJOC v0.15.0 — Expanded ETSI JOC Profile & Carriage Support

OpenJOC v0.15.0 substantially expands the public ETSI JOC profile and carriage
surface. Admission remains bounded and fail-closed, and the release keeps
synthetic, real-core, and real-full-stream evidence separate.

## Highlights

- ETSI Table 47/48 `joc_dmx_config_idx` 0 through 4 is validated, including
  Flat-7.X, 5.X+2, and the phase-signaling variants. Reserved idx5 through idx7
  are rejected.
- General E-AC-3 JOC accepts I0 with ordered D0 through D7 dependents and
  groups legal 1-, 2-, 3-, and 6-block syncframes into 1,536-sample units.
- The constrained ETSI CMAF E-AC-3 JOC path validates `ec-3`/`dec3` metadata,
  fragmented samples, and complete sample boundaries.
- The supported legacy-core path accepts original-syntax AC-3 I0 with E-AC-3
  D0 JOC. Ordinary AC-3 and non-JOC E-AC-3 remain on stock LAV/FFmpeg paths.

## Playback and integration

The Windows LAV integration keeps E-AC-3 passthrough precedence and admits only
positively confirmed JOC. Select an output policy supported by the downstream
renderer or device. LAV Stereo is compatibility Stereo, not binaural/HRTF
spatialization, and OpenJOC does not infer the physical endpoint's speaker
count.

## Validation

The release includes synthetic end-to-end coverage for idx0 through idx4,
General multi-dependent assembly, short-block grouping, legacy-core carriage,
and constrained CMAF carriage. CMAF fixture payloads are byte-exact, and raw
ES versus CMAF decoded PCM is bit-identical where the fixture expects parity.

Real-media evidence is common-profile evidence for idx0, valid-tail/core
evidence for idx1, and partial core evidence for idx4. Rare profiles and CMAF
remain synthetic-only unless stated otherwise; no healthy real full-stream claim
is made for idx4.

## Known limitations

Flat-7.X explicit `Lb/Rb` does not automatically fold into a 5.1 route;
unsupported semantic routes fail closed. Reconstructed ADM is an
interoperability representation, not the original authored master or identity.
Lossless round-trip and exact native-renderer perceptual equivalence are not
guaranteed. Legacy-core mixed MP4 is outside the standard CMAF claim, and
malformed streams remain rejected.

## Downloads and installation

Use the assets attached to the GitHub release. The Windows LAV package installs
side-by-side with stock LAV; run its `install.bat`, require `verify.bat` to
report PASS, and follow the included PotPlayer quick start. The corresponding
source archive is published with the Windows LAV binary package.
