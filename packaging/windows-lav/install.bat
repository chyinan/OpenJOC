@echo off
setlocal DisableDelayedExpansion
set "OPENJOC_STATUS=%TEMP%\OpenJOC-LAV-install-%RANDOM%-%RANDOM%.status"
if exist "%OPENJOC_STATUS%" del /q "%OPENJOC_STATUS%" >nul 2>&1
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install.ps1" -LauncherStatusPath "%OPENJOC_STATUS%" %*
set "OPENJOC_EXIT=%ERRORLEVEL%"
if exist "%OPENJOC_STATUS%" (
  del /q "%OPENJOC_STATUS%" >nul 2>&1
  exit /b %OPENJOC_EXIT%
)
echo.
echo ============================================================
echo INSTALLATION FAILED BEFORE THE INSTALLER COULD START
echo PowerShell could not start the OpenJOC installer.
echo Suggested action: Extract a fresh ZIP and try again.
echo Your organization may also block PowerShell scripts.
echo ============================================================
if /I "%~1"=="-NonInteractive" exit /b 20
echo.
pause
exit /b 20
