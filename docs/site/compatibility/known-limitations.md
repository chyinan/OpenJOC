# Known limitations

This is the canonical current list of user-visible limitations and non-claims.
Positive support status belongs to the [capability matrix](../project/capabilities.md).
Historical limitations remain in the changelog, research record, and archive.

Some of these boundaries describe areas where more evidence could help. See
[Open Problems & Contribution Opportunities](../project/open-problems.md) if
you want to contribute without treating an unresolved point as an invitation
to make a speculative fix.

OpenJOC is an independent experimental interoperability project. It does not
claim Dolby endorsement, certification, a licensed implementation,
bit-identical Reference Player output, or proprietary renderer fidelity.

## Decoder and semantic boundary

- `ObjectScene` keeps `ReconstructionBasis` rows separate from authored
  objects. The exact admitted decoded-JOC/OAMD carrier profile may report
  `SemanticBindingState::ResolvedWithinCarrier`, but that state is not
  authored-object identity recovery; all other profiles remain `Unresolved`.
- The ordinary-domain `JocSpatialBridge` renders decoded Base/RB contributions
  with OAMD-derived control, but it does not resolve authored-object identity
  or the codec-domain operator `T(t)`.
- For the exact admitted common profile (one leading Base LFE, no Bed or ISF,
  and 15 dynamic JOC objects), physical 2.0 output uses the E-AC-3
  compatibility presentation and does not add a second reconstructed-object
  Stereo contribution. The existing Lo/Ro or Lt/Rt matrix, its normative
  `1 / max_sum` overflow scaling, metadata-gated LFE policy, dialnorm, DRC, and
  FinalLinkedGain order remain unchanged. Other profiles do not inherit this
  ownership rule.
- `ETSI_STRICT` rejects syntax outside the published profile, including the
  observed reserved OAMD warp value `raw=3`. `OBSERVED_VENDOR_COMPAT` is an
  explicit, partial policy that preserves opaque continuation without
  assigning vendor semantics. One exact 15-object compatibility shape with
  raw3 is admitted for the decoded-object scene path because no additional
  raw3-specific transform was required within the tested scope.
- Public-syntax coding-tool support has bounded numerical and state coverage.
  Full fidelity for every producer, carrier, coding-tool combination, and
  malformed-input interaction is not claimed.

## Input and streaming

- JOC rendering is 48 kHz. Raw E-AC-3 and seekable ordinary ISO BMFF input
  are admitted within the documented topology and access-unit boundaries.
  Non-seekable or fragmented MP4 is not admitted.
- The Rust `OpenJocSession` packet API accepts one complete General JOC access
  unit per push: I0 plus ordered D0..D7 dependents, within the bounded public
  maximum. Demuxing, arbitrary byte fragmentation, and multiple AUs per call
  belong to the bounded C stream decoder or framework adapters, not that Rust
  packet contract.
- CMAF Annex E.3 remains limited to I0 plus optional D0. The existing original
  AC-3 Annex-J I0 combination also remains D0-only; Type 2 streams do not gain
  dependent-substream support.
- Standards-defined flat-7.X is identified narrowly by JOC downmix index 1,
  seven JOC inputs, and `L R C Ls Rs Lrs Rrs` Table-47 assembly. JOC
  reconstruction consumes the assembled I0+D0..Dn seven-input plane, while 2.0
  compatibility rendering consumes the independent I0 presentation. OpenJOC
  does not invent direct Lrs/Rrs-to-Stereo coefficients.
- The original-syntax I0 profile is narrowly scoped to the public 48-kHz
  Annex-J/TS-103-420 shape: one CRC-valid AC-3 I0, one E-AC-3 D0, matching
  six-block timing, valid semantic chanmap, and JOC/OAMD in the last D0.
  Ordinary AC-3 remains outside OpenJOC admission. General multi-dependent
  validation is transparent synthetic coverage; no redistributable real
  multi-dependent sample is claimed. A known malformed
  real-world file is still rejected at its truncated AU0 EMDF boundary; it is
  not evidence of full real-stream validation.
- Some seekable container and compatible-base workflows require `ffprobe` or
  `ffmpeg`; OpenJOC is not a zero-dependency distribution.

## Speaker and binaural rendering

- Presets and custom layouts share one generic renderer. Custom geometry is
  limited to 64 ordered output channels and at least two usable full-range
  directions. Renderer admission does not prove that a host, device, or
  container can transport the same geometry.
- `7.1.6` and the `9.1` family require semantic CAF output because their
  identities cannot be represented truthfully by a standard
  WAVEFORMATEXTENSIBLE mask. `22.2` and custom WAV use explicit unmasked PCM;
  CAF is preferred when coordinates must be preserved.
- OpenJOC performs no crossover, bass management, room correction, speaker
  calibration, head tracking, distance model, Doppler, or device discovery.
  LFE ownership is explicit and no physical device is inferred from a channel
  count.
- Binaural output is virtual-speaker rendering, not proprietary direct-object
  binaural parity. The bundled SADIE II dataset is generic; a custom SOFA may
  be more appropriate for a listener.
- Custom SOFA support is a strict local `SimpleFreeFieldHRIR` NetCDF classic
  CDF-1 subset with fixed listener pose, two receivers, common sample rate,
  and bounded exact/interpolated directional coverage. HDF5/NetCDF-4,
  resampling, downloads, writing, moving sources, and universal dataset
  coverage are not supported.

## Output level and synchronization

- DRC applies encoded E-AC-3 dynamic-range metadata. Dialnorm controls
  programme calibration. They are separate from each other and from
  file-export normalization.
- `DialnormMode::Default` is the recommended calibrated behavior. `Digital`
  explicitly selects encoded digital calibration; `Analog` is an advanced
  unity-gain compatibility/diagnostic policy, not a higher-quality or
  mastering mode.
- `--normalize-peak` applies one static post-render sample-peak scalar. It is
  not LUFS or true-peak normalization, a limiter, compressor, or DRC, and an
  inter-sample peak may exceed the requested value.
- Speaker output reports 609 samples of availability delay (577 QMF/Base-RB
  plus 32 FinalLinkedGain). Binaural reports 577 samples, excluding its finite
  FIR tail. Logical PTS is not shifted to hide this delay.

## ADM interoperability

- `export-adm` writes a reconstructed RIFF/RF64 ADM BWF representation, not
  the original ADM/BWF master. Original names, hierarchy, UIDs, authored
  binding, and discarded source information cannot be recovered. The report
  keeps `original_authored_identity_recovered: false`,
  `original_adm_master_recovered: false`, and `lossless_round_trip: false`.
- For the exact clean-room profiles (15 JOC objects, no bed, one leading Base
  LFE, no ISF, 15 dynamic OAMD objects, 16 total), decoded JOC PCM is paired
  with the corresponding OAMD dynamic metadata by typed carrier-local
  ordinals. This includes the ordinary strict profile and the exact observed
  raw3-compatible profile. Generated ADM names are neutral `OpenJOC
  Reconstructed JOC Object NN`; the report keeps original authored identity
  and original ADM-master recovery explicitly false.
- Structural and decoded-scene validation does not guarantee perceptually
  identical localization to a native JOC final renderer. A residual
  localization difference was observed in at least one real-world validation
  programme after the applicable technical checks passed; the observation is
  material-specific and non-generalizable. Native JOC playback remains the
  reference where exact native-renderer localization is required.
- A moving reconstructed Object represents the spatial metadata retained and
  decoded from the JOC programme. Its trajectory may differ from the original
  DAW automation because JOC can quantize metadata, reorganize object
  representation, change numbering, or discard authoring information. A
  meaningful decoded trajectory is not recovery of the original master.
- OpenJOC does not promise recovery of original DAW/Logic track identity,
  authored Object numbering, Object names, source-stem PCM, unquantized
  automation, programme/content hierarchy, authoring metadata, Dolby
  authoring provenance, or a lossless JOC-to-ADM round trip.
- The scoped dynamic path exports position at decoded OAMD event boundaries.
  Active/inactive transitions, extent, gain, divergence, channel lock, zones,
  and other properties are not used to invent ADM semantics. Unsupported
  metadata falls back to neutral best-effort output with a reason, or rejects
  strict export.
- When Base LFE exists, the exporter creates the minimum legal 5.1 transport
  bed. Only LFE carries recovered Base LFE PCM; L, R, C, Ls, and Rs are
  deterministic silence placeholders reported as generated structure.
- The generated `dbmd` contains the public EBU Supplement 6 envelope only.
  Reserved Atmos-specific segment payloads and Dolby authoring provenance are
  not copied, guessed, or forged.
- Logic Pro imports the reconstructed file, and a Logic-authored re-export is
  accepted by Dolby Encoding Engine. Direct DEE ingest of the byte-exact
  OpenJOC-authored file remains unsupported and unclaimed.

## APIs and integrations

- C ABI 1.4 is experimental during the OpenJOC 0.x line. The public header,
  structure sizes, ownership rules, numeric statuses, and compatibility
  initializers are the contract; ABI evolution remains possible.
- The external FFmpeg bridge is an embedding surface, not an out-of-tree
  plugin for an installed `ffmpeg` executable. The native `libopenjoc`
  decoder requires a patched custom FFmpeg build and explicit positive JOC
  selection.
- GStreamer uses an OpenJOC-specific experimental caps feature and requires a
  matching host runtime. It does not change an installed GStreamer globally.
- mpv and OpenJOC Player Bundles are project-provided custom builds, not
  official upstream mpv or FFmpeg releases. Physical multichannel playback
  still requires an audio output and device that accepts the requested map.
- The Windows DirectShow/LAV integration positively admits JOC, leaves
  ordinary E-AC-3 on stock LAV/FFmpeg, and preserves passthrough precedence.
  Its fixed 48 kHz IEEE-float PCM policies are Stereo, 5.1, 7.1, 5.1.2,
  5.1.4, 7.1.2, and 7.1.4. Each makes one exact semantic
  `WAVEFORMATEXTENSIBLE` proposal with no fallback. Automatic downstream
  semantic layout discovery is `AUTO_NOT_RELIABLE`; Stereo is the default and
  other layouts require explicit selection. Physical multichannel hardware is
  not verified. OpenJOC does not infer layouts from endpoint names, perform
  Bass Management, or translate physical subwoofer counts into logical LFE
  channels. Standalone 7.1.6/9.1.x/22.2 or custom renderer support is not a LAV
  output claim.

## Platform and release scope

- Platform packages cover the targets recorded in the current release
  metadata. Multichannel PCM generation and transport are qualified; physical
  speaker-system playback has not been independently validated on every
  Linux or Windows device.
- macOS artifacts are ad-hoc signed where required, not Developer-ID signed
  or notarized. Linux compatibility is bounded by the recorded glibc/runtime
  baseline. Windows bundles use their documented adjacent-DLL or isolated LAV
  installation models.
- Private/commercial programme fixtures and derived PCM are not distributed.
  Some real-media acceptance therefore remains a maintainer release gate.

For the corresponding positive claims and evidence boundaries, see
[capability matrix](../project/capabilities.md) and [speaker rendering](../using/speaker-rendering.md).
