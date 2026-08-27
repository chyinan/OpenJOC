# OAMD

OAMD is the object audio metadata carried alongside the JOC reconstruction data. OpenJOC decodes the supported metadata prefix and timeline into a renderer-independent scene representation.

## Profiles

- `ETSI_STRICT` follows the published ETSI constraints and rejects reserved syntax.
- `OBSERVED_VENDOR_COMPAT` is a partial, explicit compatibility policy. It preserves opaque continuation and records deviations without assigning vendor semantics.
- `AUTO` selects the appropriate supported policy for the command context; it never silently turns strict rejection into compatibility acceptance.

The observed raw OAMD `warp=3` value is represented as `ReservedWarpMode` under strict validation. One exact raw3-compatible decoded-object profile is admitted for the scoped carrier-local binding path; that does not resolve the raw3 semantic meaning for the general bridge.

## Position timeline

OpenJOC retains decoded position events in the sample domain. Multiple updates remain distinct, and a discontinuity resets timing state rather than applying stale metadata to the next segment.

For reconstructed ADM export, supported finite OAMD room coordinates are converted once at the ADM boundary. The exporter does not reuse the OAMD coordinate convention as if it were ADM Cartesian space.

## Unsupported metadata

Gain, extent, divergence, channel lock, zones, active/inactive transitions, and opaque/additional data are not converted into invented ADM semantics. Depending on policy and profile, unsupported state produces neutral best-effort output with a reason or strict rejection.

See the [capability matrix](../project/capabilities.md) for admission status and [reconstructed ADM semantics](../using/reconstructed-adm-semantics.md) for the export boundary.
