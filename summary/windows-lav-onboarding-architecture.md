# OpenJOC v0.11 Windows LAV onboarding architecture

The v0.10 package scripts are frozen inside the published/audit artifact and
are not canonical tracked templates. The v0.11 source template is
`packaging/windows-lav/`; the release packager overlays it onto the binary
staging tree.

Architecture:

- Root `install.bat`, `verify.bat`, and `uninstall.bat` are thin, quoted
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File` launchers.
- `OpenJoc.Onboarding.Core.psm1` is the functional core for runtime inventory,
  exact CLSIDs, Windows argument quoting, path equality, and path safety.
- `OpenJoc.Onboarding.Shell.psm1` owns UI, logging, elevation, native process
  execution, registry snapshot/restore, verification, and transactional
  install/uninstall.
- Default install root is `%ProgramFiles%\OpenJOC\LAV\0.11.0`; machine-wide
  COM registration does not point into Downloads or a user-writable app-data
  directory.
- Exit codes: 0 success, 10 cancellation, 20 preflight, 30 install/register,
  40 verification, 50 uninstall/rollback.

Identity and stock-isolation constraints:

- OpenJOC main CLSID: `{27247580-C701-40CD-886D-E618FC8C9FFF}`.
- Stock main CLSID: `{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}`.
- The AX also registers four property-page CLSIDs shared with stock LAV.
  Install snapshots all six relevant keys, restores pre-existing shared keys
  after registration, persists the baseline, and verifies it. Uninstall
  snapshots current non-OpenJOC shared keys before `DllUnregisterServer` and
  restores them afterward.
- Recursive deletion is centralized and requires either a valid
  `openjoc-install.json` ownership record or an exact transient marker.

PotPlayer remains manual: `POTPLAYER-QUICKSTART.md` documents adding the
registered OpenJOC filter without changing stock LAV or player settings.
