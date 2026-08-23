# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 1

**Goal:** Define one immutable, testable mapping from an explicit OpenJOC output policy to its exact logical PCM semantics without changing runtime output.

**Architecture:** A pure LAV-side contract table records both OpenJOC semantic labels and the corresponding FFmpeg/Windows representation. Lookup accepts only a stable enum; it has no device-name, filename, carrier-count, consumer-notation, or Auto input.

**Tech Stack:** C++17, FFmpeg `AVChannel`, OpenJOC C ABI, MSVC/v143, Rust canonical layout tests.

**Scope:** 6 phases from the original design; this file is phase 1 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC1: Output policy is explicit and stable

- **openjoc-lav-multichannel-output.AC1.4 Failure:** Carrier channel count, endpoint/product display name, physical subwoofer count and filename never select or alter the render target.

### openjoc-lav-multichannel-output.AC2: Every candidate preserves canonical logical semantics

- **openjoc-lav-multichannel-output.AC2.1 Success:** Stereo, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2 and 7.1.4 map to the exact OpenJOC order, count and Windows mask recorded in the canonical layout table.
- **openjoc-lav-multichannel-output.AC2.2 Success:** PCM interleave order equals ascending set-bit WAVEFORMATEXTENSIBLE order, so admitted layouts require no silent reorder.
- **openjoc-lav-multichannel-output.AC2.3 Failure:** A layout without an exact canonical Windows mask is excluded; zero masks, reserved bits and count-only defaults are rejected.
- **openjoc-lav-multichannel-output.AC2.4 Failure:** A consumer `.2` subwoofer notation never creates a second logical LFE; physical subwoofer routing remains downstream.

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->
<!-- START_TASK_1 -->
### Task 1: Build the canonical output-policy contract with RED/GREEN tests

**Verifies:** openjoc-lav-multichannel-output.AC1.4, openjoc-lav-multichannel-output.AC2.1, openjoc-lav-multichannel-output.AC2.2, openjoc-lav-multichannel-output.AC2.3, openjoc-lav-multichannel-output.AC2.4

**Files:**

- Create: `D:\Program\LAVFilters-OpenJOC\include\LAVOpenJocSettings.h` (`// pattern: Functional Core` ABI contract)
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutput.h`
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutput.cpp`
- Create/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutputTests.cpp` (unit)

**Implementation:**

1. Write the test first. It must require exactly seven policies—Stereo, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2, and 7.1.4—and reject invalid enum values, Auto, 5.2.4, 7.1.6, 9.x, and 22.2.
2. Compile before creating the contract implementation. Expected RED: `C1083` or unresolved symbols for `LAVOpenJocSettings.h`, `OpenJocOutput.h`, or contract lookup.
3. Define `enum class LAVOpenJocOutputPolicy : uint32_t` with fixed wire values `Stereo = 0`, `Layout51 = 1`, `Layout71 = 2`, `Layout512 = 3`, `Layout514 = 4`, `Layout712 = 5`, and `Layout714 = 6`; there is no Auto value. Add `LAV_OPENJOC_OUTPUT_POLICY_SCHEMA_VERSION = 1`, `static_assert(sizeof(LAVOpenJocOutputPolicy) == sizeof(uint32_t))`, and tests that pin every numeric value so persistence/COM ABI cannot drift silently. Define `LAVOpenJocOutputContract` with policy, ABI preset name, OpenJOC layout name, FFmpeg standard layout name, OpenJOC semantic labels, ordered `AVChannel` values, channel count, FFmpeg mask, and Windows mask.
4. Implement a static immutable table and enum-only lookup. Returned pointers must remain stable for the process lifetime. Mark both runtime-bearing headers and sources with `// pattern: Functional Core`.
5. Preserve the verified naming distinction: OpenJOC `5.1` means `FL FR FC LFE Ls Rs`, while the FFmpeg standard name is `5.1(side)`; map `Ls/Rs` to `AV_CHAN_SIDE_LEFT/RIGHT` and `Lb/Rb` to `AV_CHAN_BACK_LEFT/RIGHT` instead of comparing label spellings.
6. Pin the Stereo row explicitly: property-page display label `Stereo`, OpenJOC semantic/ABI layout name `2.0`, FFmpeg standard layout name `stereo`, order `FL FR`, and mask `0x3`. Display text is never reused as a semantic identifier.

**Testing:**

- Verify every exact count/order/mask listed in the design.
- Verify FFmpeg mask equals Windows mask, mask popcount equals count, and ordered channels equal ascending set bits.
- Verify every candidate has exactly one logical LFE; `.2` in 5.1.2/7.1.2 is TFL/TFR, not a second LFE.
- Verify zero masks, reserved/unmapped bits, count-only defaults, and unknown policies produce no contract.
- Verify no parsing API exists for endpoint names, consumer notation, carrier count, or filenames.
- Verify the schema version, fixed enum values, and `uint32_t` representation with compile-time and executable assertions.

**Verification:**

Run from a VS 2022 x64 developer environment:

```text
cl /nologo /EHsc /std:c++17 /I"D:\Program\LAVFilters-OpenJOC\include" /I"D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio" /I"D:\Program\LAVFilters-OpenJOC\ffmpeg" "D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutputTests.cpp" "D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutput.cpp" /Fe:"D:\Program\OpenJOC\.codex-tmp\OpenJocOutputTests.exe"
D:\Program\OpenJOC\.codex-tmp\OpenJocOutputTests.exe
```

Expected GREEN: compile succeeds and the executable exits 0.

**Commit:** `feat(audio): define canonical OpenJOC output contracts`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Integrate and cross-check the contract against OpenJOC canonical layouts

**Verifies:** openjoc-lav-multichannel-output.AC2.1, openjoc-lav-multichannel-output.AC2.2, openjoc-lav-multichannel-output.AC2.3, openjoc-lav-multichannel-output.AC2.4

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj`
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj.filters`
- Modify: `D:\Program\OpenJOC\scripts\release_lav_smokes.cmd`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_release_lav_smokes_script.py` (unit)
- Create/Test: `D:\Program\OpenJOC\scripts\tests\LavSmokeNoopLifecycle.cpp` (temporary command-line lifecycle source used until Phase 5 replaces it)
- Test existing: `D:\Program\OpenJOC\crates\openjoc-scene\tests\speaker_layouts.rs` (unit)

**Implementation:**

Add `OpenJocOutput.cpp` to the LAVAudio project with precompiled headers disabled for this pure unit, and add `OpenJocOutputTests` to the existing command-line smoke build rather than inventing a new test project. Extend the script-structure test to require the new test source and to retain all existing smoke commands. Add the minimal checked-in no-op lifecycle source so the entire smoke script has a reproducible fifth input before the Phase 5 graph lifecycle executable exists.

**Verification:**

```powershell
cargo test -p openjoc-scene --test speaker_layouts public_presets_have_the_admitted_names_and_backend_contracts -- --exact
python -m unittest scripts.tests.test_release_lav_smokes_script
& 'D:\Program\OpenJOC\scripts\release_lav_smokes.cmd' `
  'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat' `
  'D:\Program\LAVFilters-OpenJOC' `
  'D:\Program\OpenJOC\crates\openjoc-capi\include' `
  'D:\Program\OpenJOC\scripts\tests\LavSmokeNoopLifecycle.cpp' `
  'D:\Program\OpenJOC\.codex-tmp\phase01-smokes'
& 'D:\Program\OpenJOC\scripts\release_lav_msbuild.cmd' `
  'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat' `
  'D:\Program\LAVFilters-OpenJOC' `
  'D:\Program\OpenJOC\crates\openjoc-capi\include' `
  'D:\Program\OpenJOC\.codex-tmp\phase01-lav-msbuild.log'
```

Expected: Rust canonical test passes, the full smoke script compiles every command-line target including `OpenJocOutputTests`, script tests pass, and `D:\Program\LAVFilters-OpenJOC\bin_x64\LAVAudio\LAVAudio.ax` builds. Run `release_lav_smokes.cmd` after every later command-line smoke addition. Do not change the OpenJOC renderer.

**Commit:** `test(audio): integrate OpenJOC output contract checks`
<!-- END_TASK_2 -->
<!-- END_SUBCOMPONENT_A -->
