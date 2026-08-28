# Reconstructed ADM renderer equivalence

OpenJOC reconstructs an interoperability-oriented ADM representation of the decoded JOC object scene. It does not recover the original authored Dolby Atmos master, and it does not guarantee perceptually identical localization when a generic ADM renderer renders that file.

If you want to investigate this boundary, see the corresponding [open problem
and contribution guidance](../project/open-problems.md) before proposing a
renderer-semantic change.

## Validated within scope

The current validation boundary covers:

| Boundary | State |
| --- | --- |
| Decoded object PCM scene | Validated within the tested scope. |
| Decoded JOC Object ↔ OAMD mapping | Validated for the admitted carrier-local profile. |
| OAMD-to-ADM coordinate conversion | Validated. |
| Position interpolation and event timing | Validated for the exported profile. |
| Physical BW64 / `chna` / TrackUID mapping | Validated 15/15 in the tested real-media audit. |
| Public renderer-state coverage | `COMPLETE_WITH_SCOPE`. |

These checks establish decoded-scene and ADM-structure correctness. They do not establish that a generic ADM renderer will make the same final localization choices as a native JOC renderer.

## Remaining guarantee

A self-authored real-world validation programme exhibited a residual localization difference after the applicable decoded-scene, mapping, coordinate, timing, and file-structure checks passed. That observation is material-specific and non-generalizable. It is enough to keep exact perceptual equivalence outside the guarantee.

Use native JOC playback as the reference when exact native-renderer localization matters. Do not treat the ADM exporter as fundamentally broken: its contract is reconstructed interoperability, not native renderer emulation.

The same boundary explains why `ResolvedWithinCarrier` is not authored identity. See [Decoded Objects vs authored Objects](../concepts/decoded-vs-authored-objects.md) and [Reconstructed ADM export](../using/reconstructed-adm-export.md).
