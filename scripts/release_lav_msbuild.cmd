@rem SPDX-FileCopyrightText: 2026 OpenJOC contributors
@rem SPDX-License-Identifier: GPL-2.0-or-later
@rem pattern: Imperative Shell
@echo off
setlocal

if "%~4"=="" (
  >&2 echo Usage: release_lav_msbuild.cmd VSDEVCMD LAV_ROOT OPENJOC_INCLUDE LOG_FILE
  exit /b 64
)

call "%~1" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%

call msbuild "%~2\LAVFilters.sln" ^
  /t:LAVAudio:Rebuild /m ^
  /p:Configuration=Release ^
  /p:Platform=x64 ^
  /p:EnableOpenJOC=true ^
  /p:EnableOpenJOCSideBySide=true ^
  "/p:OpenJocIncludeDir=%~3" ^
  /p:BuildProjectReferences=true ^
  /nologo /v:minimal /fl ^
  "/flp:LogFile=%~4;Verbosity=minimal"
exit /b %errorlevel%
