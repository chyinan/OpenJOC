@echo off
setlocal DisableDelayedExpansion
echo This experimental package changes 64-bit LAV Audio, Video and Splitter registration.
echo Video/Splitter replace stock LAV CLSIDs; audio replaces prior OpenJOC and shared pages.
echo Close players. Preserve previous complete packages, registered paths and settings.
echo Read README.md before continuing. No installed files will be copied or removed.
choice /C YN /N /M "Explicitly unregister ALL candidate filters at this package path? [Y/N] "
if errorlevel 2 exit /b 10
if errorlevel 1 goto approved
exit /b 10
:approved
set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if defined PROCESSOR_ARCHITEW6432 set "PS=%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
"%PS%" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\manual-registration.ps1" -Operation Unregister
set "RESULT=%ERRORLEVEL%"
echo Review the result above before closing this window.
pause
exit /b %RESULT%
