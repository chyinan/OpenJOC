# E-AC-3 JOC overview


E-AC-3 JOC object decoding starts with a supported E-AC-3 carrier. OpenJOC extracts and parses its in-band EMDF, then decodes the supported OAMD prefix/timeline and JOC reconstruction payload separately. JOC reconstruction uses decoded E-AC-3 channel audio and emits carrier-local `ReconstructionBasis` PCM rows.

```text
supported E-AC-3 JOC access units
  +-- E-AC-3 frames/audio blocks -----------------> base/channel PCM
  +-- in-band auxiliary data ---------------------> EMDF extraction and parse
        +-- OAMD payload -------------------------> supported prefix/timeline
        |                                               -> decoded spatial metadata
        +-- JOC payload + decoded base/channel PCM -> JOC reconstruction
                                                        -> carrier-local ReconstructionBasis PCM rows

decoded Base PCM + OAMD metadata + reconstruction rows
  +-- render-joc -> Base/RB PCM + OAMD bridge control
  |                 -> experimental, bounded speaker/binaural paths
  +-- export-adm -> exact JOC/OAMD binding gate
                    -> decoded JOC object PCM + supported OAMD movement
                    -> reconstructed ADM BWF
```

This is OpenJOC's conceptual processing model, not a normative Dolby decoder diagram. OAMD metadata and JOC reconstruction rows are separate decoder outputs. The documented binding gate pairs them as decoded JOC Objects only for admitted profiles; other profiles remain unbound. That association does not recover authored Object identity, source-stem PCM, unquantized automation, or the original Atmos master.

The `render-joc` speaker/binaural path uses the experimental JOC spatial bridge. Its codec-domain operator `T(t)` remains unresolved; binaural output virtualizes the supported speaker field and is not a direct-object or native-renderer equivalence claim. Reconstructed ADM export has its own exact binding and validation scope.

For implementation ownership and state boundaries, see [OpenJOC architecture](architecture.md). For the object-identity boundary, see [Decoded Objects vs authored Objects](decoded-vs-authored-objects.md). For the status and evidence behind each claim, see the [capability matrix](../project/capabilities.md).
