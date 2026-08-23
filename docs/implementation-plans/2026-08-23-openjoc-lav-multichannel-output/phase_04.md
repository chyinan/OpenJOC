# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 4

**Goal:** Persist and expose OpenJOC output policy through an isolated COM/settings surface while the property page shows only layouts admitted by shipped evidence.

**Architecture:** A new `ILAVOpenJocSettings` interface is parallel to—not an extension of—`ILAVAudioSettings`. Programmatic validation may select every representable contract; a separate shipped-layout table drives the UI and initially contains only Stereo until Phase 6 evidence exists.

**Tech Stack:** C++17 COM, Windows Registry, DirectShow property pages/resources, MSVC/v143, command-line COM and registry smoke tests.

**Scope:** 6 phases from the original design; this file is phase 4 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC1: Output policy is explicit and stable

- **openjoc-lav-multichannel-output.AC1.1 Success:** A new filter defaults to Stereo and produces the same OpenJOC configuration and two-channel float output as the released path.

### openjoc-lav-multichannel-output.AC6: Settings and evidence are honest

- **openjoc-lav-multichannel-output.AC6.1 Success:** The existing property page exposes only Stereo and presets admitted by the shipped validation evidence, under the isolated OpenJOC registry namespace.
- **openjoc-lav-multichannel-output.AC6.2 Success:** Programmatic settings use an OpenJOC-specific interface without changing the stock `ILAVAudioSettings` ABI.

---

<!-- START_SUBCOMPONENT_A (tasks 1-3) -->
<!-- START_TASK_1 -->
### Task 1: Add the independent COM settings interface and isolated persistence

**Verifies:** openjoc-lav-multichannel-output.AC1.1, openjoc-lav-multichannel-output.AC6.2

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\include\LAVOpenJocSettings.h` (`// pattern: Functional Core` fixed ABI declaration)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.h` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.cpp` (`// pattern: Imperative Shell`)
- Create/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocSettingsSmoke.cpp` (integration)
- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudioIdentitySmoke.cpp` (integration)
- Modify: `D:\Program\OpenJOC\scripts\release_lav_smokes.cmd`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_release_lav_smokes_script.py` (unit)

**Implementation:**

1. RED: require `QueryInterface(IID_ILAVOpenJocSettings)` and set/get/default/invalid/round-trip behavior. Current target must return `E_NOINTERFACE`; stock control must continue doing so after implementation.
2. Add `ILAVOpenJocSettings : IUnknown` under fixed IID `{6B97FD1C-B463-4B5E-9349-CD8B964D6B46}` with get/set policy methods. Do not add, delete, or reorder any member of `ILAVAudioSettings`; preserve its IID `{4158A22B-6553-45D0-8069-24716F8FF171}`.
3. Expose the interface only for the OpenJOC side-by-side build in `NonDelegatingQueryInterface`. The setter validates through the Phase 1 contract table, holds the receive lock, invokes the one Phase 2 policy-change path, clears incompatible compressed/pending/strict queue state, and returns `E_INVALIDARG` without state change for invalid values.
4. Default to Stereo. Persist two DWORDs only below the existing `Software\LAV\Audio\OpenJOC` namespace: `OpenJocOutputPolicyVersion = 1` and the fixed `uint32_t` `OpenJocOutputPolicy`. Load only when the version is exactly supported and the value resolves through the contract table; missing, future/unknown version, invalid value, wrong registry type, or truncated data leaves Stereo. Runtime config must not write the registry.
5. Mark the pure public ABI header `// pattern: Functional Core`; mark COM/registry/runtime mutations and stateful declarations `// pattern: Imperative Shell`.

**Testing:**

- Use `RegOverridePredefKey` and a test-only volatile subtree with RAII restoration.
- First instance defaults Stereo; every representable policy set/gets; invalid enum preserves state; 7.1.4 survives release/recreation.
- Pin every persisted DWORD value and cover missing/v0/future schema versions, invalid policy, wrong type, and a v1 round-trip; compile-time tests retain the four-byte ABI size.
- The value exists only below the isolated OpenJOC key.
- Old settings IID remains callable in both builds; new IID works only in the OpenJOC target.
- A policy change causes the Phase 2 decoder recreation observable on subsequent output.

**Verification:** Run settings/identity smokes, script tests, the complete `release_lav_smokes.cmd`, and target/stock builds. Expected: all pass and registry override is restored.

**Commit:** `feat(audio): add isolated OpenJOC output settings interface`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Drive the property page from a separate shipped-evidence table

**Verifies:** openjoc-lav-multichannel-output.AC6.1

**Files:**

- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocShippedLayouts.h` (`// pattern: Functional Core`)
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocShippedLayouts.cpp`
- Create/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocShippedLayoutsTests.cpp` (unit)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\AudioSettingsProp.h` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\AudioSettingsProp.cpp` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.rc` (`// pattern: Imperative Shell` UI resource)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\resource.h`
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj`
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj.filters`

**Implementation:**

1. RED shipped-layout tests require an initial list containing exactly Stereo, no Auto, and no unproven preset.
2. Add `// pattern: Functional Core` to both shipped-table header/source, separate from the seven-row representable contract table. Phase 6 is the only phase allowed to add evidence-proven rows.
3. Add a dedicated OpenJOC output combo to the existing audio settings page; do not reuse Mixer `IDC_OUTPUT_SPEAKERS`. Populate it by enumerating shipped enum values and resolving display text through the canonical table; store the enum with `CB_SETITEMDATA`. Never parse display text, endpoint names, or product names.
4. If a programmatically selected validation-only policy is absent from the shipped list, show no selection and do not silently write Stereo on Apply.
5. Guard the UI with `LAV_OPENJOC_SIDE_BY_SIDE`. Add `$(OpenJocDefines);$(OpenJocIdentityDefines);%(PreprocessorDefinitions)` to `ResourceCompile` in Debug and Release so the `.rc` guard matches `ClCompile`; stock resource layout remains unchanged. Mark stateful property-page header/source and the guarded resource as `// pattern: Imperative Shell`.
6. In the target-only LAV Audio Status page, use the connected filter instance passed to `OnConnect` (not a separately created helper instance) to query `ILAVOpenJocSettings::GetOutputPolicy`, `ILAVOpenJocStatus::GetOpenJocAdmissionState`, and the existing `ILAVAudioStatus::GetOutputDetails`. Display policy, admission state, output format, rate, channel count, and mask. This is host-independent filter diagnostics used later to prove what the actual PotPlayer graph instance adopted; the stock status page remains byte-for-byte unchanged.

**Testing:**

- Shipped list is Stereo-only and every row resolves to a valid canonical contract.
- No Auto enum/string and no name parser are present.
- Target property page round-trips Stereo; stock page has no new controls.
- With a live target instance, the status page reports the selected fixed enum, `OpenJoc` admission after JOC samples, and the exact float32/48 kHz/count/mask returned by that same instance; an ordinary E-AC-3 control reports `StockEac3`.
- A legal mask, fake `QueryAccept`, or representable contract alone cannot add a row.

**Verification:** Run shipped-list tests, target property-page smoke, target build, stock resource/build control. Expected: target shows only Stereo; stock is unchanged.

**Commit:** `feat(audio): expose only evidenced OpenJOC layouts`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Close Phase 4 with ABI and build-order regression checks

**Verifies:** openjoc-lav-multichannel-output.AC1.1, openjoc-lav-multichannel-output.AC6.1, openjoc-lav-multichannel-output.AC6.2

**Files:**

- Test all files produced in Phases 1–4; no new production file is introduced.

**Implementation:**

Run OpenJOC target tests/build, then the stock identity/build control, then rebuild the OpenJOC target so the phase-ending artifact is the requested filter. Inspect the `ILAVAudioSettings` declaration diff to prove its vtable is byte-for-byte unchanged. Confirm registry tests restored predefined keys and that the shipped list remains Stereo-only.

**Verification:** All admission, decoder, output-contract, strict-output, settings, identity, shipped-list, script, target-build, and stock-control checks pass.

**Commit:** `test(audio): verify isolated OpenJOC settings surface`
<!-- END_TASK_3 -->
<!-- END_SUBCOMPONENT_A -->
