# Clean-room methodology

OpenJOC is an independent clean-room implementation. The implementation boundary is governed by source class, not by whether an observation was useful.

## Permitted implementation inputs

- public normative specifications and official public companion tables;
- public mathematics and DSP literature;
- sanitized behavioral specifications containing only implementation-necessary rules, numerical contracts, state behavior, and acceptance tests.

The implementation boundary is deliberately separate from analysis evidence:

```text
authorized analysis
        ↓
governance and sanitization
        ↓
behavioral clean-room specification
        ↓
independent implementation
```

## Prohibited implementation inputs

Production implementation must not use proprietary source code, decompiler or disassembler output, assembly, private symbols or addresses, proprietary structure layouts, copied implementation expressions, or another decoder's source code.

Controlled black-box observations and private fixtures, when explicitly authorized, remain analysis evidence. They do not become implementation provenance merely because a related behavior appears in a test.

## Evidence classes

| Class | Meaning |
| --- | --- |
| `NORMATIVE / PUBLIC EVIDENCE` | Public specifications, official tables, public API/layout definitions, and public mathematics that may directly support implementation. |
| `CONTROLLED / CONTAMINATED ANALYSIS EVIDENCE` | Isolated observations used to determine behavior that public sources do not settle. These are not direct implementation inputs. |
| `BEHAVIORAL CLEAN-ROOM SPECIFICATION` | Sanitized functional behavior, contracts, constants, and tests that can cross into independent implementation. |

## How this affects documentation

The [capability matrix](capabilities.md) records current status and evidence boundaries. Dated experiments and negative results stay in the repository's `docs/research/` history. They are not silently promoted into user-facing capabilities.

The repository retains the full [implementation provenance record](https://github.com/chyinan/OpenJOC/blob/master/docs/PROVENANCE.md) for detailed component history. This page is the public operating summary; the full record is not part of the site's navigation.

OpenJOC is not affiliated with, endorsed by, or sponsored by Dolby Laboratories. Clean-room status is an engineering provenance description, not a legal guarantee.
