# E-AC-3 JOC overview

E-AC-3 JOC carries a base E-AC-3 programme together with metadata and reconstruction data. OpenJOC treats those pieces as separate decoder-domain inputs before composing an output path.

```text
E-AC-3 JOC carrier
        │
        ├── E-AC-3 core ───────────────► base programme PCM
        │
        ├── JOC ReconstructionBasis ──► decoded object PCM rows
        │
        └── OAMD ──────────────────────► decoded object metadata
                                             │
                                             ▼
                                  OpenJOC carrier-local scene
                                      /          |          \\
                                  speaker     binaural    ADM export
```

This is OpenJOC's conceptual processing model. It is not a normative Dolby decoder architecture diagram, and it does not imply access to authored Objects or a native renderer's hidden state.

The scene has two separate output branches:

- `render-joc` uses the experimental JOC spatial bridge and the selected speaker or binaural renderer;
- `export-adm` uses the scoped decoded-object/OAMD binding gate and writes an interoperability-oriented ADM representation.

The bridge's codec-domain operator `T(t)` remains unresolved. That status is independent from the exact carrier-local binding profile used by reconstructed ADM export.

For implementation ownership and state boundaries, see [OpenJOC architecture](architecture.md). For the object-identity boundary, see [Decoded Objects vs authored Objects](decoded-vs-authored-objects.md).
