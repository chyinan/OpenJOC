# Open Problems & Contribution Opportunities

OpenJOC is usable today. It can decode supported E-AC-3/JOC input, render
speaker or binaural output, and export an interoperability-oriented ADM
representation within the documented scope.

This page describes a small set of problem areas where new public evidence,
careful validation, or narrowly scoped engineering could materially improve
the project. It is not a bug graveyard, a v0.14 wishlist, or a list of promises.
The status of each item is deliberately explicit so that contributors do not
have to repeat research that has already reached a stop condition.

## At a glance

| Topic | Type | Difficulty |
| --- | --- | --- |
| Native-renderer equivalence for reconstructed ADM | Open research / known limitation | Very high |
| Reconstructed PCM headroom and ADM storage | Engineering / standards interoperability | High |
| Physical multichannel hardware validation | Validation wanted | Medium |

## Native-renderer equivalence for reconstructed ADM

!!! warning "Status: open research / known limitation"
    **Difficulty:** Very high

### Why it matters

OpenJOC reconstructs an interoperability-oriented representation of the
decoded JOC object scene. That representation is useful for inspection,
exchange, and rendering through a generic ADM toolchain. It is not a recovered
copy of the original authored Dolby Atmos master.

When a generic ADM renderer plays the reconstructed file, its final perceptual
localization is not guaranteed to match the final localization produced by a
native JOC renderer. Closing that gap would require evidence about a renderer
semantic that the current public contract does not establish.

### Current evidence and boundary

Within the current tested scope, OpenJOC has validated:

- decoded object PCM;
- decoded JOC Object ↔ OAMD binding for the admitted carrier-local profile;
- OAMD-to-ADM coordinate conversion;
- dynamic position timing and interpolation;
- gain and state handling for tested material;
- BW64, `chna`, and TrackUID mapping; and
- public per-object renderer-state coverage, recorded as
  `COMPLETE_WITH_SCOPE`.

Those checks establish a decoded-scene and ADM-structure contract. They do not
establish native-renderer equivalence. At least one self-authored real-world
programme showed a residual localization difference even after the applicable
checks passed. That observation is material-specific and must not be turned
into a generalized claim such as “vocals are 30 degrees left.”

The related [renderer-equivalence limitation](../compatibility/renderer-equivalence.md)
and [decoded-object identity boundary](../concepts/decoded-vs-authored-objects.md)
describe what the current implementation does and does not claim.

### Useful contribution

A high-value contribution could be any of the following:

- new **public specification evidence** that closes a remaining final-render
  semantic;
- a narrowly controlled, reproducible experiment that distinguishes two
  explicit renderer hypotheses;
- a standards-backed ADM representation for a proven missing semantic; or
- a reproducible cross-renderer test that materially reduces the remaining
  candidate space.

A useful result does not have to include a production fix. A rigorous negative
result can be just as valuable when it shows that a public specification does
not define the suspected semantic or that a controlled hypothesis fails.

### What would count as progress

Progress means an independently repeatable result with a precise problem
statement, a stated evidence boundary, and a falsifiable conclusion. A result
should explain which observable behavior changes, which renderer hypothesis it
supports or rejects, and why the conclusion does not depend on one programme.

### What not to submit

Do not submit any of the following as a semantic fix:

- empirical position offsets;
- arbitrary gain tweaks;
- hard-coded compensation for one song;
- “move everything X degrees” rules;
- source-specific fingerprinting;
- undocumented vendor-derived formulas; or
- copied proprietary implementation code.

“Make this sample sound centered” is not sufficient evidence for a production
change. Any renderer-semantic change must be justified independently of one
test programme and must respect the [clean-room methodology](clean-room-methodology.md).

### Recommended first step

For a difficult renderer question, open a GitHub Discussion with the exact
observable problem, the public material being used, the proposed competing
hypotheses, and the smallest reproducible test. Do that before investing in a
large implementation or pull request.

## Reconstructed PCM headroom and ADM storage

!!! warning "Status: open engineering / standards interoperability problem"
    **Difficulty:** High

### Why it matters

JOC reconstruction is performed in floating point. Some legitimate
reconstructed samples can exceed the normalized range directly representable
by signed 24-bit PCM. The project must preserve signal meaning without making
an undocumented choice that changes the programme.

### Current evidence and boundary

The current ADM export path fails closed when a sample is non-finite or outside
the signed 24-bit range. It does not clip, saturate, normalize, limit, or
silently attenuate the programme. This is intentional: an out-of-range
reconstructed sample is not, by itself, evidence that the decoder produced
corrupt audio.

The [PCM24 headroom page](../compatibility/pcm24-headroom.md) documents the
current behavior. The open question is how to represent such a valid floating
reconstruction in a standards-compliant, genuinely interoperable ADM/BW64
workflow. Do not assume that a file extension, a metadata field, or one tool's
ability to open a file proves interoperability.

### Useful contribution

Useful work would establish a storage strategy that:

- preserves the reconstructed signal semantics;
- avoids silent clipping and silent normalization;
- remains compatible with the required ADM/BWF profile; and
- is demonstrated with real-world ADM tooling, not only a writer-side parser.

The investigation may compare standards-valid sample or container
representations, but it should not prescribe a solution before public
specifications and target-tool behavior support it.

### What would count as progress

Progress requires a public standards/profile argument, a reproducible fixture,
read/write validation, and an interoperability result from the actual target
toolchain. The report should state whether absolute level, object relationships,
and ADM structure are preserved, and which cases remain unsupported.

### What not to submit

Do not submit a change whose only justification is one of these:

- clamp every sample to `[-1, 1]`;
- normalize every export; or
- apply a fixed attenuation to make the writer accept the file.

Those changes silently alter the signal unless an independently proven
standards/profile semantic justifies them. The current fail-closed behavior is
preferable to silently changing the output.

### Recommended first step

Start with a public standards and interoperability report. Include the exact
sample/container representation under test, the target tools, the expected
read-back behavior, and a small reproducible fixture. Keep the default PCM24
policy unchanged until the evidence closes the storage question.

## Physical multichannel hardware validation

!!! note "Status: validation wanted"
    **Difficulty:** Medium

### Why it matters

This is a lower-barrier contribution path for people who do not want to work
on codec or renderer semantics. OpenJOC has substantial bounded Windows
transport and endpoint-path evidence, but physical multichannel and height
speaker coverage remains incomplete. A successful software negotiation or a
virtual endpoint is not proof that a physical speaker system reproduces the
same channel map.

The current documentation does not claim physical multichannel playback on
every Linux or Windows device, driver, host, or output API. Please do not turn
one successful setup into a platform-wide support statement.

### Useful contribution

A useful validation report should include:

| Field | Record |
| --- | --- |
| System | OS and version |
| OpenJOC | Release, commit, or package build |
| Hardware | Audio interface, receiver, or sound card |
| Driver | Driver name and version |
| Host | Player or application |
| Output API | WASAPI, DirectSound, ALSA, or other API |
| Configuration | Selected layout and channel map |
| Physical setup | Speaker count and physical placement |
| Test material | Deterministic channel or sweep signal, plus source type |
| Result | Expected behavior and observed behavior |
| Evidence | Logs, negotiated format, and relevant screenshots where useful |

For speaker-channel validation, prefer deterministic mapping signals or
repeatable test tones over a subjective statement that the result “sounds
right.” Validation-only contributions are useful even when they document a
device or API rejection.

### What would count as progress

Progress is a report another person can reproduce and compare. It should make
clear which part passed: format negotiation, PCM delivery, channel order,
physical routing, or all of them. A failed device/API combination is still
valuable when the failure is precise and repeatable.

### What not to submit

Do not claim general hardware support from a single subjective listening test,
an endpoint name, or a successful virtual-device path. Do not omit the selected
layout, channel order, driver, or host details. Do not attach private programme
material or machine-specific evidence that cannot be shared safely.

### Recommended first step

Choose one explicit layout and one deterministic test signal. Record the full
configuration above, then share the result in a Discussion or a focused
documentation/validation pull request. A major host or transport change still
needs the ordinary evidence and regression bar.

## Boundaries to preserve

The simple “add Base full-band PCM directly to the final scene” hypothesis has
not been established. The current public final-scene contribution remains
**inconclusive**, and an omission bug is **not confirmed**. Do not reopen that
hypothesis as a likely fix without new public evidence and a controlled test
that distinguishes it from double-counting or other scene-assembly effects.

Likewise, `ResolvedWithinCarrier` describes a bounded decoded-JOC/OAMD binding
inside the carrier. It does not recover authored identity, source-stem PCM, or
the behavior of a proprietary renderer.

## Evidence and clean-room requirements

OpenJOC does not accept production implementations copied from proprietary
source code or derived by directly transcribing proprietary implementation
logic. Read the [clean-room methodology](clean-room-methodology.md) before
working on codec or renderer semantics.

For a semantic or high-risk engineering change, a useful contribution should
normally include:

- a reproducible test;
- an exact statement of the observable problem;
- the boundary of the evidence;
- public specification evidence where available;
- independently derived behavioral evidence where appropriate; and
- regression coverage for the claimed behavior.

Controlled observations may help identify a question, but they are not by
themselves permission to copy a proprietary implementation or to present a
vendor behavior as a public standard.

## Start with a Discussion for difficult research

For difficult research topics, open a GitHub Discussion before starting a
large implementation or pull request. This helps surface existing stop
conditions, avoid duplicated research, align on evidence, and keep review
focused.

You do not need prior permission for a typo fix, straightforward documentation
change, or small deterministic bug with a clear regression test. The
Discussion-first recommendation applies to high-risk semantic research, not to
ordinary maintenance.

## What a useful pull request explains

A research or semantic pull request should answer:

1. What observable problem is being fixed?
2. What evidence establishes the expected behavior?
3. Which existing hypothesis or limitation does the change close?
4. What regression test demonstrates the change?
5. What does the change **not** claim?

Large unexplained patches and tuning constants without evidence are difficult
to review. AI-assisted contributions are welcome when their evidence,
provenance, and reproducibility meet the same bar as any other contribution.

## Negative results are useful

You do not have to “fix something” for the work to matter. A rigorous negative
result can show that:

- a public specification does not define the missing semantic;
- a candidate hypothesis fails a controlled test; or
- a device or API demonstrably rejects a format.

State the test conditions, evidence boundary, and falsifier clearly. Such a
result can save the next contributor from repeating an unproductive path.

For ordinary contribution workflow and repository rules, see
[Contributing](contributing.md).
