# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 6

**Goal:** Decide shipped layouts solely from measured native renderer and PotPlayer Source-as-Output streaming evidence, then publish only truthful claims.

**Architecture:** A machine-validated evidence schema separates `STREAM_PROVEN`, `UNSUPPORTED`, and `UNVERIFIED`. Native exact-renderer runs prove DirectShow negotiation; PotPlayer runs prove host integration under the same renderer/endpoint. The shipped table and documentation are derived from, and checked against, only `STREAM_PROVEN` rows.

**Tech Stack:** Python evidence validator/tests, native DirectShow harness, PotPlayer 64-bit, Windows audio renderer, C++ shipped-layout model, Markdown documentation.

**Scope:** 6 phases from the original design; this file is phase 6 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`. Host inventory: Windows 11 25H2 build 26200.9168, PotPlayer 26.07.01.0, active multichannel virtual endpoint semantics unproven, physical HDMI AVR inactive.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC1: Output policy is explicit and stable

- **openjoc-lav-multichannel-output.AC1.5 Failure:** Auto is not exposed or documented as supported without standards-based semantic preference evidence across stereo, 5.1 and one height-capable downstream.

### openjoc-lav-multichannel-output.AC3: DirectShow negotiation is exact and strict

- **openjoc-lav-multichannel-output.AC3.2 Success:** A layout is reported supported only after exact connection, exact `ConnectionMediaType`, Pause/Run and sample delivery in the named host/renderer environment.

### openjoc-lav-multichannel-output.AC6: Settings and evidence are honest

- **openjoc-lav-multichannel-output.AC6.1 Success:** The existing property page exposes only Stereo and presets admitted by the shipped validation evidence, under the isolated OpenJOC registry namespace.
- **openjoc-lav-multichannel-output.AC6.3 Success:** The final matrix distinguishes `STREAM_PROVEN`, `UNSUPPORTED` and `UNVERIFIED` and records the exact failure stage/HRESULT where applicable.
- **openjoc-lav-multichannel-output.AC6.4 Failure:** Documentation never claims automatic physical-device adaptation or physical speaker playback without corresponding evidence.

---

<!-- START_SUBCOMPONENT_A (tasks 1-4) -->
<!-- START_TASK_1 -->
### Task 1: Define and test the machine-verifiable support-evidence rules

**Verifies:** openjoc-lav-multichannel-output.AC1.5, openjoc-lav-multichannel-output.AC3.2, openjoc-lav-multichannel-output.AC6.3, openjoc-lav-multichannel-output.AC6.4

**Files:**

- Create: `D:\Program\OpenJOC\scripts\lav_multichannel_evidence_core.py`
- Create: `D:\Program\OpenJOC\scripts\validate_lav_multichannel_evidence.py`
- Create/Test: `D:\Program\OpenJOC\scripts\tests\test_lav_multichannel_evidence.py` (unit)
- Create: `D:\Program\OpenJOC\docs\integration\evidence\windows-lav-multichannel-2026-08-23.json`
- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocShippedLayoutsTests.cpp` (`--list-shipped`)

**Implementation:**

1. RED validator tests reject empty evidence, legal-mask-only, `QueryAccept`-only, changed connection type, missing samples, and any mandatory Stereo/5.1/7.1 row that is not proven. Initial shipped Stereo-only output must mismatch any additional proven row until Task 3.
2. Mark `STREAM_PROVEN` only when the same named real renderer has a successful exact `ConnectDirect`, identical requested/pre/post connection types, Pause and Run states, raw and MP4 delivered samples/EOS, no fallback/mutation/error, plus a successful PotPlayer Source-as-Output run on the same renderer/endpoint.
3. Mark `UNSUPPORTED` only after an actual exact test rejects or mutates the type; store failure stage and HRESULT. Mark insufficient/unexecuted rows `UNVERIFIED` with a reason; never convert an unrun layout into unsupported.
4. Store OpenJOC/LAV HEADs and binary hashes, OS/host/version/hash, renderer moniker and endpoint ID, fixture kind/hash, logical layout/count/mask, requested and connected formats, stage HRESULTs, sample/byte/EOS/lifecycle counters, final status, and failure stage. Native rows must also store the absolute runtime module paths/hashes asserted by `PrivateComModule` and process enumeration; PotPlayer rows must store in-process paths/hashes observed after graph creation for LAV Audio, LAV Splitter, `openjoc_capi.dll`, every loaded FFmpeg LAV DLL, and `libbluray.dll`. Missing, duplicate-basename, wrong-path, or wrong-hash dependency evidence fails validation. Manifest hashes without runtime-loaded-module verification fail validation.
5. For all seven candidates record `logical_lfe_channels: 1`; provide no physical-subwoofer-count field and no friendly-name-derived semantics. Mark evidence-rule code `# pattern: Functional Core`.

**Testing:** Synthetic evidence fixtures cover all three states and every false-positive rule. `--list-shipped` is compared exactly to `STREAM_PROVEN` rows. Auto is absent.

**Verification:** Python validator unit tests pass; the real empty/pre-run matrix correctly fails the mandatory Stereo/5.1/7.1 gate.

**Commit:** `test: validate multichannel support evidence`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Collect native renderer and PotPlayer Source-as-Output evidence

**Verifies:** openjoc-lav-multichannel-output.AC3.2, openjoc-lav-multichannel-output.AC6.3, openjoc-lav-multichannel-output.AC6.4

**Files:**

- Populate: `D:\Program\OpenJOC\docs\integration\evidence\windows-lav-multichannel-2026-08-23.json`
- Create evidence artifacts under: `D:\Program\OpenJOC\docs\integration\evidence\windows-lav-multichannel-2026-08-23\`
- Create/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocPolicyControl.cpp` (private-activation persistent-policy helper; `// pattern: Imperative Shell`)
- Modify: `D:\Program\OpenJOC\scripts\test_lav_directshow_negotiation.cmd`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_lav_directshow_negotiation_script.py`

**Implementation:**

1. Before mutation, snapshot OpenJOC COM registration, both versioned OpenJOC policy DWORDs, PotPlayer relevant settings/filter registry, selected renderer, and endpoint. Own restoration with an outer RAII/process-finally guard and verify byte-for-byte restoration after the matrix; do not change the default audio endpoint.
2. Build `OpenJocPolicyControl.exe` from the same public settings header and private-activation helper used by the native harness. `--set-persistent POLICY TARGET_LAVAUDIO_AX` privately activates that exact module, calls `ILAVOpenJocSettings::SetOutputPolicy` in persistent (non-runtime-config) mode, releases it, recreates another private instance, and requires get-policy plus the v1 registry DWORDs to match. `--get` reports the same information. Before every PotPlayer raw or MP4 row—including policies absent from the Stereo-only UI—run `--set-persistent`, record its output, and after PotPlayer creates the graph run `--get` again. This is the exact selection mechanism for all seven validation policies; no combo-box text or endpoint/product name is parsed.
3. Stage/register the branch-built target with adjacent exact dependencies. Confirm in the visible PotPlayer UI that Audio Output Channels is “Source (Input) as Output”; do not infer that label from registry index `AudSpkIndex_new=22`. After playback begins, open the LAV Audio Status property page belonging to the actual PotPlayer graph instance and capture its target-only policy/admission fields plus its existing output format/rate/channel-count/mask fields. Require policy equal the requested fixed enum, admission equal `OpenJoc`, and actual output equal float32/48 kHz with the target contract's exact count/mask. Independently use process-module enumeration to require the loaded LAV Audio/Splitter, `openjoc_capi.dll`, every loaded FFmpeg LAV DLL, and `libbluray.dll` absolute paths/hashes to equal the staged branch manifest; a registered CLSID or helper `--get` alone is not sufficient.
4. Use the full moniker `@device:cm:{E0F158E1-CB04-11D0-BD4E-00A0C911CE86}\DirectSound:{97AC8CB2-E6E9-41B4-ADB2-6A23C785EBBE}` as the current named real renderer test target. The text `CABLE In 16 Ch` and its mask-zero endpoint properties are inventory only, never semantic proof.
5. For each candidate, run native renderer-moniker raw+MP4 exact connections and PotPlayer raw+MP4 under the same renderer. Exercise Pause/Run, forward/back seek, EOS, and stop/reopen. Native rows inherit the asserted private-activation/dependency paths/hashes. PotPlayer evidence records the actual graph instance's status-page policy/admission/output contract, playback progression, selected persistent policy before/after graph creation, and observed in-process dependency paths/hashes; native runs supply the explicit `ConnectDirect` proof. Validator tests must reject a PotPlayer row that has only registry/helper evidence, lacks same-instance `OpenJoc` admission, or whose actual format/rate/count/mask differs from the contract.
6. Record each row immediately. A rejection or mutation is `UNSUPPORTED`; do not retry with a different mask, format, speaker geometry, renderer, or channel count and then call it supported. Insufficient execution remains `UNVERIFIED`.
7. Stereo, 5.1, and 7.1 are all mandatory: if any one is not `STREAM_PROVEN`, do not claim completion and report a material blocker. Do not add any failed row to the UI. Height-layout exact rejection is explicitly `UNSUPPORTED`; incomplete height testing is `UNVERIFIED`.
8. Use the `computer-use` skill for visible PotPlayer interaction and screenshots, announcing any skill-caused pause. Mark the collection driver `# pattern: Imperative Shell` if code is added. After the final run unregister/stage-clean only the temporary registration, restore every snapshot, and prove the original registered module, PotPlayer settings, and OpenJOC policy bytes are restored.

**Verification:** Every candidate has one of the three states with complete supporting fields; every PotPlayer run proves same-instance policy/admission/output details and every run proves actually loaded filter/dependency paths/hashes; COM/PotPlayer/settings bytes are restored. The complete `release_lav_smokes.cmd` includes the policy-control build/test. Mandatory gate passes only if Stereo, 5.1, and 7.1 are genuinely `STREAM_PROVEN`.

**Commit:** `test: record PotPlayer multichannel evidence`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Admit and document only STREAM_PROVEN rows

**Verifies:** openjoc-lav-multichannel-output.AC1.5, openjoc-lav-multichannel-output.AC6.1, openjoc-lav-multichannel-output.AC6.3, openjoc-lav-multichannel-output.AC6.4

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocShippedLayouts.cpp`
- Modify: `D:\Program\OpenJOC\README.md`
- Modify: `D:\Program\OpenJOC\docs\CAPABILITIES.md`
- Modify: `D:\Program\OpenJOC\docs\KNOWN_LIMITATIONS.md`
- Modify: `D:\Program\OpenJOC\docs\JOC_RENDER.md`
- Modify: `D:\Program\OpenJOC\docs\integration\LAV_FILTERS_OPENJOC.md`
- Modify: `D:\Program\OpenJOC\packaging\windows-lav\POTPLAYER-QUICKSTART.md`
- Modify: `D:\Program\OpenJOC\scripts\repository_hygiene_core.py`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_repository_hygiene.py` (unit)
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_lav_multichannel_evidence.py` (unit)

**Implementation:**

1. RED: evidence may contain proven rows while Phase 4 shipped table remains Stereo-only; require `SHIPPED_LAYOUT_MISMATCH`. Existing stereo-only documentation checks must also fail against the new truthful evidence model.
2. Add only evidence `STREAM_PROVEN` rows to `OpenJocShippedLayouts.cpp`; keep unsupported/unverified rows in the matrix and out of the combo.
3. Document Stereo default, explicit manual presets, `AUTO_NOT_RELIABLE`, exact masks/formats/failure HRESULTs, and the environment-specific three-state matrix. Never call a controlled fake-sink pass renderer support.
4. Explicitly state that consumer `.2` subwoofer notation does not create a second logical LFE and that physical subwoofer distribution, crossover, delay, level, phase, EQ, room correction, and multi-sub optimization remain downstream.
5. Replace hardcoded “stereo-only” hygiene with evidence-link, three-state, no-Auto, logical/physical-subwoofer separation, and shipped-equals-proven gates. Preserve archived 0.10/0.11 release docs.
6. PotPlayer quickstart requires Source-as-Output but never instructs parsing device names.

**Verification:** Shipped-list output equals the evidence `STREAM_PROVEN` set exactly; validator, repository hygiene, and documentation tests pass; no unsupported/unverified row appears as supported.

**Commit:** `feat: admit only stream-proven OpenJOC layouts`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Run final cross-validation and produce the completion report

**Verifies:** openjoc-lav-multichannel-output.AC1.5, openjoc-lav-multichannel-output.AC3.2, openjoc-lav-multichannel-output.AC6.1, openjoc-lav-multichannel-output.AC6.3, openjoc-lav-multichannel-output.AC6.4

**Files:**

- Verify all Phase 1–6 production, test, evidence, and documentation files; no new feature scope.

**Implementation:**

Run, in order: all LAV unit/smoke tests; controlled strict-sink raw/MP4 matrix; actual renderer exact-connect matrix; PotPlayer Source-as-Output matrix; ordinary E-AC-3 target-vs-stock; passthrough precedence for every policy; lifecycle/renegotiation; allocator/performance; OpenJOC target build; stock build/identity; final OpenJOC rebuild; all OpenJOC script tests; repository hygiene; evidence validator; independent code review. Use the `verification-before-completion` and `requesting-code-review` skills before any completion claim.

**Verification:** Controlled sink is reported only for that environment. Actual renderer support requires native exact-renderer streaming; PotPlayer support additionally requires same-instance host policy/admission/output proof and streaming. Any failed exact test remains failed without fallback/hack. If Stereo, 5.1, or 7.1 is not proven, report a material blocker rather than completion.

**Commit:** `test: finalize OpenJOC multichannel validation matrix`
<!-- END_TASK_4 -->
<!-- END_SUBCOMPONENT_A -->
