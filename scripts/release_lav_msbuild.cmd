@rem SPDX-FileCopyrightText: 2026 OpenJOC contributors
@rem SPDX-License-Identifier: GPL-2.0-or-later
@rem pattern: Imperative Shell
@echo off
setlocal

if "%~4"=="" (
  >&2 echo Usage: release_lav_msbuild.cmd VSDEVCMD LAV_ROOT OPENJOC_INCLUDE LOG_FILE [audio^|full]
  exit /b 64
)

rem Keep existing four-argument callers on the audio-only release path.
set "LAV_VARIANT=%~5"
if "%LAV_VARIANT%"=="" set "LAV_VARIANT=audio"
if /i not "%LAV_VARIANT%"=="audio" if /i not "%LAV_VARIANT%"=="full" (
  >&2 echo Unsupported LAV variant: %LAV_VARIANT%
  exit /b 64
)
set "LAV_BUILD_TARGET=/t:LAVAudio:Rebuild"
set "LAV_TOOLSET_OPTIONS="
if /i "%LAV_VARIANT%"=="full" (
  rem Rebuild the complete solution, including all three filters and QuickSync.
  set "LAV_BUILD_TARGET=/t:Rebuild"
  set "LAV_TOOLSET_OPTIONS=/p:PlatformToolset=v145 /p:WindowsTargetPlatformVersion=10.0"
)

call "%~1" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%

call msbuild "%~2\LAVFilters.sln" ^
  %LAV_BUILD_TARGET% /m ^
  %LAV_TOOLSET_OPTIONS% ^
  /p:Configuration=Release ^
  /p:Platform=x64 ^
  /p:RegisterOutput=false ^
  /p:EnableOpenJOC=true ^
  /p:EnableOpenJOCSideBySide=true ^
  "/p:OpenJocIncludeDir=%~3" ^
  /p:BuildProjectReferences=true ^
  /nologo /v:minimal /fl ^
  "/flp:LogFile=%~4;Verbosity=minimal"
exit /b %errorlevel%
