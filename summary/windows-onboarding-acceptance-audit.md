# OpenJOC 0.11 Windows onboarding acceptance audit

Date: 2026-08-23 (Asia/Shanghai)

## Final artifact and frozen boundary

- Final onboarding QA ZIP: `<local-audit-path>\OpenJOC-LAV-0.11.0-onboarding-QA.zip`
- ZIP SHA-256: `DEDE4E4CEA8AD4BF3FEA7C50519C631EB6F7CEC4D88EB891B46A351097A3767D`
- Canonical onboarding surface: 10/10 files hash-identical between `packaging\windows-lav` and the fresh ZIP extraction.
- Published v0.10 ZIP SHA-256 remains `68150AB6A2C4494AD82A5AF9CF1445EC057E815EE646140B5C54DE6FC9EB9B4A`.
- No publication was performed.

## Automated evidence

- `python -m unittest discover -s scripts/tests`: 73 tests passed, 9 skipped only because external build/source evidence variables were not configured.
- Windows PowerShell 5.1 BAT verification: 9/9 PASS, exit 0, including with an intentionally invalid `PSModulePath`.
- PowerShell 7 direct verification: 9/9 PASS, exit 0.
- Path matrix covers spaces, parentheses, Unicode, ampersand, apostrophe, and exclamation mark.
- Failure tests cover missing package/runtime files, corrupt PE/runtime payload, registration failure with rollback, UAC cancellation, unsafe ownership/deletion paths, invalid registry baselines, logging failure before and after commit, and stable exit codes.
- Final code review: no remaining Critical or Important findings.

## Real-host Explorer and UAC evidence

- User double-clicked final-package `install.bat` from Explorer, accepted UAC, and observed success. Final log: `OpenJOC-LAV-install-20260823-013405-835.log`, exit 0.
- User double-clicked `verify.bat` from Explorer and observed 9/9 PASS. Explorer verify log: `OpenJOC-LAV-verify-20260823-011953-096.log`, exit 0.
- User double-clicked `uninstall.bat` from Explorer, confirmed removal, accepted UAC, and observed success. Explorer uninstall log: `OpenJOC-LAV-uninstall-20260823-012039-456.log`, exit 0.
- A real UAC cancellation produced exit 10, ordinary-language cancellation text, and a console that stayed open for acknowledgement.
- Repeated install succeeded; repeated absent uninstall returned `NOTHING TO REMOVE`, exit 0, without UAC; install after uninstall succeeded.
- After uninstall, all six registry exports matched the authoritative pre-v0.11 snapshots byte-for-byte. The main CLSID returned to `%LOCALAPPDATA%\OpenJOC\0.10.0\LAVAudio.ax`; stock K-Lite and four shared property CLSIDs were unchanged.
- Final installed state contains 42 files; the committed transaction rollback directory is absent; the saved uninstall baseline is complete, hash-valid, and points to v0.10 rather than the installed v0.11 path.

## PotPlayer evidence

- PotPlayer 64-bit opened a real E-AC-3 Dolby Digital Plus + Dolby Atmos/JOC sample and remained responsive.
- In the installed state, its process loaded all six expected v0.11 OpenJOC modules: `LAVAudio.ax`, `openjoc_capi.dll`, `avcodec-lav-63.dll`, `avformat-lav-63.dll`, `avutil-lav-61.dll`, and `swresample-lav-7.dll` from `%ProgramFiles%\OpenJOC\LAV\0.11.0`.
- In the uninstalled state, the same playback loaded K-Lite stock `LAVAudio.ax` and `avcodec-lav-62.dll`, proving stock playback remained functional.
- `POTPLAYER-QUICKSTART.md` gives the short manual Filter Priority flow and does not modify PotPlayer automatically.

## Release boundary

The QA ZIP proves the onboarding surface against the frozen v0.10 runtime base. It is intentionally not represented as a publish-ready v0.11 release candidate because the overlaid base still contains v0.10 release/provenance documents. A future authorized v0.11 release must stage the canonical onboarding template together with final v0.11 binaries and provenance through `scripts/release_packaging.py`, rerun release gates, and obtain separate publication authorization.
