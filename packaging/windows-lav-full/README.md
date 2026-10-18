# OpenJOC LAV full 0.20.1 for Windows x64 (optional test bundle)

This is the explicit `full` package variant. The default audio package remains
`openjoc-lav-0.20.1-windows-x64.zip`, with its existing one-click `install.bat`,
`verify.bat`, and `uninstall.bat` experience. Choose that default package for the
normal OpenJOC audio installation. This separate full bundle does not replace it.

This bundle contains OpenJOC-enabled LAVAudio, LAVVideo, LAVSplitter (including
Splitter Source), IntelQuickSyncDecoder, and a matching x64 runtime dependency
set. Keep every file in `runtime/` together. Do not mix filters or DLLs from
other builds. Packaging does not establish playback or hardware-decoding support.

## Test without registering first

Extract into a new permanent folder, separate from existing installations.
Close all media players before switching filters. Preserve your previous
complete LAV package, exact registered paths, and player/LAV settings.
When a 64-bit player supports loading external filters directly by file, prefer
selecting each candidate `.ax` from this package's `runtime/` folder. Check the
active filter paths during playback. Player setup is a separate step: this
bundle does not select player filters or run an installer automatically.

## Optional manual registration

Read this section before running either batch file. Run `register-all.bat` as
administrator only if you deliberately want system-wide 64-bit registration of
all three filters. The batch file asks for explicit confirmation. Registration
is in-place: do not move or delete this folder afterward. No installed files
are copied over or removed, but DirectShow selection/merit changes can affect
other players.

LAVVideo and both LAVSplitter classes use standard LAV CLSIDs, so registering
this bundle replaces existing stock/K-Lite LAV Video/Splitter registrations.
OpenJOC audio uses its existing OpenJOC CLSID, replacing an earlier OpenJOC audio
registration. Some audio property-page CLSIDs are shared with stock LAV audio
and may change too. Existing LAV settings can be shared. Save previous paths
and settings yourself; there is no automatic backup.

`unregister-all.bat` also asks for confirmation. It refuses if any known class
points to a different module path, checking 64-bit per-user and machine
registrations, including property pages. Missing registrations are allowed.
The path guard is not a registry snapshot or protection against concurrent
edits. Never use another package's uninstall script to remove this bundle.

The helper verifies the complete `PACKAGE_SHA256SUMS.txt` inventory before
loading native code. It waits for each `regsvr32` process and checks its actual
exit code, stopping on a missing or nonzero result. A failure can still leave
earlier filters registered or unregistered; there is no automatic rollback.
Do not disable verification or hand-edit the packaged helper to bypass a failure.

## Roll back

Close players. Run this bundle's `unregister-all.bat` as administrator, then
re-register your saved previous complete package with its own documented tools
and original paths. Re-select its filters in the player and restore saved
settings if needed. Keep the old package until actual playback is verified.
If the path guard refuses, stop and inspect the current registered paths:
another installation owns at least one class. Do not force unregistration.

## Validation limits

This template reuses the corrected registration helper from the verified
`regfix1` candidate. Its Windows PowerShell 5.1 regression tests mock filter
registration and test GUI-process waiting/exit-code handling with a harmless
fixture. They do not register real filters. Synthetic packaging tests check
variant separation, required inputs, manifests, hashes, and archive contents.

Check the current build's CI results for the exact source and validation scope.
Packaging or DLL load checks alone do not verify actual registration,
unregistration, real-player playback, physical audio endpoints, video frame
decoding, demux/seek behavior, or GPU/Intel Quick Sync hardware decoding.
Source information and copied LAV/component license notices are in
`SOURCE_AND_LICENSES.md` and `LICENSES/`.
