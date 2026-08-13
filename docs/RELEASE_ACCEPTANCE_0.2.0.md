# OpenJOC 0.2.0 release acceptance

Status: `OPENJOC_0_2_0_RELEASE_GO_WITH_DOCUMENTED_LIMITATIONS`

The 0.2.0 source state passed the workspace quality gates, release-mode build,
local bundle verifier, CLI/version smoke, representative raw E-AC-3 and
ordinary seekable ISO BMFF streaming smoke, malformed-input smoke, and the
existing numerical/streaming regression suite. The historical 0.1.0 release
and tag remain preserved. No tag, push, GitHub Release, or publication action
was performed by this acceptance.

The release contract remains deliberately bounded: `SemanticBindingState` is
`Unresolved`; ReconstructionBasis rows are diagnostic decoder coordinates, not
authored-object stems; authored-object PCM, an audio-bound ObjectScene, and a
renderer are not admitted. `RcLfe` remains separate, and observed OAMD
`warp=3` remains `ReservedWarpMode` under `ETSI_STRICT` with no vendor semantic
alias. Current release validation is scoped to the documented Apple-silicon
macOS workflow and is not a Developer-ID or notarized release.

The local artifact checksum and full gate evidence are retained in the private
J4R15 acceptance manifest; publication remains a later explicit human action.
