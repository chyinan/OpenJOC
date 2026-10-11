# Optional full Windows LAV package

The default remains `audio`: `openjoc-lav-0.19.0-windows-x64.zip` keeps
`install.bat`, `verify.bat`, and `uninstall.bat`, with the existing isolated
OpenJOC Audio onboarding. It does not register stock Video or Splitter classes.

The default LAV source is pinned to
`a876fbc0ca2615fdaa240cf96ac13c82f28689a9`, including upstream FFmpeg
`b033272d7ef2070db2dc8ef228e67b20213e90c8` and the merged PCM policy fixes.

## Explicit full option

Maintainers may select `full` in the LAV workflow's dispatch inputs or pass
`--variant full --onboarding-template packaging/windows-lav-full` to
`scripts/package_lav_release.py` alongside its normal build-input arguments
and `--core-source-root` pointing to the exact sources used for the C API DLL.
The full variant requires this separate template. Tag builds retain the
audio default. The full asset has a separate `openjoc-lav-full-...` name and
contains Audio, Video, Splitter, QuickSync, and one matching dependency set.
The release workflow remains a release-publishing operation: selecting a
variant does not turn it into an artifact-only test workflow.

The full package is an advanced, explicit alternative. Its `register-all.bat`
and `unregister-all.bat` are manual controls, not replacements for the default
audio installer. Extracting the package performs no registration.

1. Close players. Record current Audio/Video/Splitter registered paths and keep
   the complete previously working package(s), including their original installers.
2. Extract the complete full ZIP into a new permanent directory. Do not mix
   dependency DLLs, copy individual filters over another installation, or move
   the directory after registration.
3. If you intend to replace the standard Video/Splitter registrations too,
   right-click `register-all.bat`, choose **Run as administrator**, and confirm
   `Y`. Typing `Y` does not itself elevate the process.
4. Require success for all three filters. A later failure can leave earlier
   filters registered; retain both directories and review the reported error.
   Verify the actual filters and paths loaded by your player independently.

The helper validates the complete package manifest, x64 process, administrator
rights, and runtime inventory before invoking registration. It waits for each
process and checks that process's exit code; it does not use `$LASTEXITCODE`
for the GUI registration executable. Unregistration checks ownership and
refuses to remove registrations that now point elsewhere.

## Rollback

Close players. If the full package still owns its registrations, run its
`unregister-all.bat` as administrator and explicitly confirm. Then use the
previous complete LAV package's original registration/installer and the
previous OpenJOC audio package's `install.bat` as appropriate. Verify the
loaded paths before removing any old directory. The default audio package's
uninstaller does not manage the full bundle's Video/Splitter registrations.

## Validation boundary

The original complete candidate built core
`ce6c3bb3e2f4f16e2cf224fc66b39585d120f14f` with the LAV pin above:
[full build and package checks](https://github.com/chyinan/OpenJOC/actions/runs/38124827977).
Its registration-only fix passed 86 Windows PowerShell 5.1 tests, including six
harmless GUI process cases, and preserved all 47 runtime files byte-for-byte:
[registration regression run](https://github.com/chyinan/OpenJOC/actions/runs/38127796553).
No test registered filters on the CI host. The user reported that this fixed
candidate appeared to work; this is limited trial feedback, not certification
of all playback paths, hardware, channel layouts, GPUs, or devices. Subsequent
core revisions and new full-package builds still need their own validation.
