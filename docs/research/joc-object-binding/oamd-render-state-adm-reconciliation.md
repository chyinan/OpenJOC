# OAMD renderer-state to ADM reconciliation

Status: complete clean-state audit; one renderer-relevant ADM profile-state gap
was proven and fixed. Its ability to correct the reported static lead-vocal
left bias still requires the human Logic recheck; that causal link is not
claimed here.

## Scope and evidence boundary

This is a clean-scene audit of public OpenJOC source, public ETSI/ADM profile
documents, the user-owned R00 source, the existing coordinate-fixed candidate,
and the maintainer's Logic observation. It does not reopen the coordinate
bridge, decoded-row binding, raw3 admission, PCM storage, authored identity, or
any proprietary renderer.

The coordinate bridge remains frozen:

```text
ADM_X = 2 * OAMD_X - 1
ADM_Y = 1 - 2 * OAMD_Y
ADM_Z = OAMD_Z
```

The machine-readable record is
[`oamd-render-state-adm-reconciliation.json`](oamd-render-state-adm-reconciliation.json).

## R00 census

The clean R00 input is `成都 ~Dolby ATMOS Test2~ .m4a`, SHA-256
`83bbaabd5705bd4a458dc8ebc7b8bef66ebf5d76d2f8d09b1cdaa225105e2156`.
The production metadata preflight decoded 10,249 access units at 48 kHz,
covering 15,742,464 samples. It found 16 OAMD objects, 15 dynamic objects,
15 ReconstructionBasis rows, and 163,984 metadata updates.

The decisive state counts are:

| State | R00 result | Consequence |
|---|---:|---|
| Active updates | 163,984 | No inactive transition is present to bias the export. |
| Gain updates | 163,984 | The field is decoded on every update. |
| Non-unity gain updates | 0 | Every decoded gain is 0 dB; gain cannot explain the vocal symptom. |
| Negative-infinity gain updates | 0 | No decoded gain-silence state is present. |
| Priority values | only 1.0 | No non-default priority state is present. |
| Ramp duration | 1536 samples on every update | The only non-default dynamic state found in the census; it exposed a target ADM profile serialization gap. |
| Dynamic position kinds | 153,735 room | Positions are already exported through the accepted coordinate bridge. |
| Size / channel lock / zone exclusion / divergence | all zero | No R00 contribution modifier is present. |
| Trim | opaque raw warp 3 | Public typed trim semantics are unavailable; no meaning is inferred. |

## Coverage conclusion

The public OAMD object state that can affect contribution includes active state,
object gain, priority/importance, position and distance/screen resolution,
timing and ramp duration, size, channel lock, zones, divergence, extended
position precision, trim, and bounded additional data. OpenJOC decodes and
resolves these fields as follows:

- The direct JOC speaker bridge consumes active state, gain scalar, resolved
  position, size, zones, channel lock, timing/ramp duration, and the bounded
  object descriptor. It does not claim authored identity.
- The scene layer preserves gain, active, priority, resolved position, size,
  zones, channel lock, divergence, trim-disabled flags, and timing.
- The admitted ADM exporter writes decoded object PCM unchanged, converts the
  resolved room position to ADM Cartesian coordinates, and emits sample-domain
  event blocks. Its Dolby profile `jumpPosition` transport is now profile
  conformant: `interpolationLength=0` on the first Object block and `250` on
  subsequent blocks. It does not write active-object gain, priority, size,
  channel lock, zones, divergence, trim, or opaque data.
- In strict mode, an inactive dynamic transition is rejected rather than
  silently presented as active. Best-effort mode reports the binding failure
  and falls back to neutral dynamic metadata.

This means the exporter has known unsupported behavior for future non-default
OAMD states. It does not mean that one of those states occurred in R00. For
R00, gain is unity, all objects are active, all contribution modifiers are at
their defaults, and the position mapping is covered by the prior coordinate
regression. No R00 semantic PCM bake is justified.

`ramp_duration=1536` is preserved into the direct bridge scheduler and is not
serialized as an OAMD-number field in ADM. The Dolby Atmos Master ADM Profile
uses discrete `jumpPosition` events and renderer-side smoothing; its profile
rule is not a direct copy of the OAMD ramp value. The pre-fix writer used
`interpolationLength=0` for every block, which violated the profile rule for
subsequent blocks. The minimal production fix emits 0 for the first block and
250 for subsequent blocks, with no PCM transformation.

## Public profile implications

ETSI TS 103 420 defines object gain as a decoder-interface level in dB and
states that the gain should be applied to the object audio essence. It also
defines `b_object_not_active` as silent object essence and defines timing
`ramp_duration` as the period for interpolating from prior to current property
values. The same public specification documents channel lock, zones,
divergence, and trim semantics.

The Dolby Atmos Master ADM Profile v1.0 constrains active Objects so that gain
is not used to express arbitrary non-unity level; inactive Objects have silent
PCM and may carry the profile's inactive gain/importance markers. The profile
also uses `jumpPosition` events with a profile-defined `interpolationLength`
rule and renderer-side smoothing. R00's all-1536-sample timing made the
profile-boundary omission observable, so the writer and validator now encode
and enforce the first-block-0/subsequent-block-250 rule. A future non-unity
OAMD gain stream would still require a separate controlled design decision:
profile-valid PCM semantic preparation or a documented rejection. R00 provides
no gain-bake trigger.

## Final disposition

`RENDER_STATE_EXPORT_GAP = CONFIRMED_FOR_DOLBY_ADM_JUMP_INTERPOLATION_STATE`.
The gap is proven as an ADM profile-conformance defect, but it is not yet
proven to be the cause of the R00 vocal-localization symptom.

The earlier Logic result remains unchanged: global topology PASS, vocal
localization FAIL on the previous coordinate-fixed candidate. The corrected
candidate requires a human Logic recheck. The remaining clean blocker is the
causal attribution of any audible change: R00's trim element is an unresolved
raw warp-3 opaque element and no independently identifiable clean vocal
stem/reference was provided for a contribution matrix. This audit does not
infer a gain, active-state, authored-identity, or proprietary-renderer
explanation.

## Conditional fix validation

The fixed strict R00 candidate passed the production export and independent
`validate-adm --json` checks. It contains 21 tracks, 21 unique CHNA UIDs,
327.968 seconds of audio, 15 bound dynamic Objects, and 991,775,232 bytes of
signed 24-bit PCM. The decoder-domain PCM census remains safe: all samples are
finite and in range, with max positive `0.7297523170899117` and min negative
`-0.8124570678959339`.

Two independent full exports are byte-identical:

- WAV SHA-256: `5AC8AA2A75F60D020889732AF800992A1323DE2CC967A9C086BB8AD6BB8DCEBF`
- `data` chunk SHA-256: `C749E57CA0B287C423FE3FC676D84F76EDC0EA337233A5F4E5F5D3EF12313857`
- XML jump counts: 15 first-block `0` values, 153,720 subsequent-block `250`
  values, and 0 unexpected values.

The candidate and a Logic checklist are recorded in
[`HUMAN_LOGIC_VOCAL_LOCALIZATION_RECHECK.md`](HUMAN_LOGIC_VOCAL_LOCALIZATION_RECHECK.md).
The previous human result remains global topology PASS / vocal localization
FAIL until that checklist is run against the corrected candidate.

Public references:

- [ETSI TS 103 420 V1.2.1](https://www.etsi.org/deliver/etsi_ts/103400_103499/103420/01.02.01_60/ts_103420v010201p.pdf)
- [Dolby Atmos Master ADM Profile v1.0](https://developer.dolby.com/globalassets/documentation/technology/dolby_atmos_master_adm_profile_v1.0.pdf)
- [ITU-R BS.2076](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2076-2-201910-S%21%21PDF-E.pdf)
