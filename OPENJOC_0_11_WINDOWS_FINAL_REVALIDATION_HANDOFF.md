# OpenJOC 0.11 final Windows revalidation handoff

## Integration state

- Branch: `codex/openjoc-0.11-integration`
- Integration source head before this handoff artifact: `224c459`
- Integration base: `33ef4bc47531b32f302443c0225b328070b9d79c`
- Arbitrary geometry source head: `3fd2c42`
- Windows onboarding source head: `a081385`
- OpenJOC C ABI: `1.4`
- Published `v0.10.0` tag: unchanged at `6530973c34daaf22f8d710e0e80a0e3de175d507`
- v0.11 tag/release/assets: not created or published

The Mac integration tree has passed the core workspace gates and source-level
Windows onboarding audit. Native Windows execution remains mandatory before
any v0.11 release authorization.

## Required final-package inputs

- Build the final OpenJOC C ABI 1.4 library from the exact integration HEAD.
- Build/package the OpenJOC-enabled LAV/DirectShow surface from the exact
  integration source and final v0.11 runtime/provenance inputs.
- Do not reuse the prior onboarding QA ZIP as the release artifact. Its evidence
  SHA-256 was `DEDE4E4CEA8AD4BF3FEA7C50519C631EB6F7CEC4D88EB891B46A351097A3767D`.
- Stage final package metadata, checksums, licenses, PE dependency closure,
  and provenance through the release packaging tools.

## Mandatory native Windows tests

- Final C ABI 1.4 build and consumer smoke.
- OpenJOC-LAV compatibility/build and PE dependency closure.
- Explorer double-click `install.bat`, automatic UAC, and cancellation UX.
- `verify.bat` PASS from Explorer and PowerShell 5.1; PowerShell 7 where available.
- PotPlayer real JOC playback, ordinary E-AC-3 behavior, passthrough precedence,
  seek/EOS/reopen, and stock LAV rollback/preservation.
- Difficult-path package smoke: spaces, parentheses, Unicode, ampersand,
  apostrophe, and exclamation mark.
- `uninstall.bat`, idempotent install/uninstall, rollback, and clean final state.
- Fresh final-package QA, including the nine reconciled external-input tests below.

The validated DirectShow/LAV output remains 48 kHz stereo float PCM. Renderer
support for custom geometry up to 64 channels must not be promoted to an
arbitrary multichannel PotPlayer/LAV claim.

## Prior 73-pass / 9-skip reconciliation

The prior Windows acceptance reported 73 passed and 9 skipped because external
build/source evidence variables were not configured. All nine are classified
as `MUST_RUN_ON_FINAL_V0_11_PACKAGE`; none may remain skipped for convenience:

| Test | Prior reason | Final classification |
| --- | --- | --- |
| `OpenJocReleaseBinaryTests.test_c_abi_exports_are_unchanged` | Reference/rebuilt DLL and `dumpbin` paths unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `OpenJocReleaseBinaryTests.test_private_source_prefixes_are_absent_and_generic_prefix_is_present` | Reference/rebuilt DLL and `dumpbin` paths unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `GccRuntimeSourceTests.test_exact_official_source_and_binary_package_metadata` | `OPENJOC_GCC_EVIDENCE_ROOT` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `GccRuntimeSourceTests.test_binary_package_runtime_matches_release_runtime` | GCC evidence/release runtime paths unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `LavReleaseNoticeTests.test_new_files_have_openjoc_copyright_and_gpl2_or_later_spdx` | `OPENJOC_LAV_SOURCE_ROOT` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `LavReleaseNoticeTests.test_modified_upstream_files_have_release_and_date_notice` | `OPENJOC_LAV_SOURCE_ROOT` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `LavReleaseNoticeTests.test_modified_source_files_retain_upstream_gpl_notice` | `OPENJOC_LAV_SOURCE_ROOT` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `LavReleaseNoticeTests.test_provenance_documents_and_machine_readable_census_are_complete` | `OPENJOC_LAV_SOURCE_ROOT` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |
| `ReleaseToolchainShimTests.test_all_cross_prefix_shims_are_licensed_and_delegate` | `OPENJOC_MSYS2_BASH` unavailable | `MUST_RUN_ON_FINAL_V0_11_PACKAGE` |

## Release blockers

- Native Windows final-package revalidation is still open.
- Final v0.11 package/provenance inputs are still open.
- No release/tag/publication action is authorized by this handoff alone.
