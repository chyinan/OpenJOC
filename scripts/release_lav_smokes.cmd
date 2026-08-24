@rem SPDX-FileCopyrightText: 2026 OpenJOC contributors
@rem SPDX-License-Identifier: GPL-2.0-or-later
@rem pattern: Imperative Shell
@echo off
setlocal

if "%~5"=="" (
  >&2 echo Usage: release_lav_smokes.cmd VSDEVCMD LAV_ROOT OPENJOC_INCLUDE LIFECYCLE_SOURCE OUTPUT_DIR
  exit /b 64
)

call "%~1" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%

pushd "%~5"
if errorlevel 1 exit /b %errorlevel%

call cl /nologo /EHsc /std:c++17 /O2 /MT ^
  "%~2\decoder\LAVAudio\OpenJocAdmissionTests.cpp" ^
  "%~2\decoder\LAVAudio\OpenJocAdmission.cpp" ^
  /Fe:OpenJocAdmissionTests.exe
if errorlevel 1 exit /b %errorlevel%

call cl /nologo /EHsc /std:c++17 /O2 /MT ^
  "/I%~2\decoder\LAVAudio" "/I%~2\include" "/I%~2\ffmpeg" ^
  "%~2\decoder\LAVAudio\OpenJocOutputTests.cpp" ^
  "%~2\decoder\LAVAudio\OpenJocOutput.cpp" ^
  /Fe:OpenJocOutputTests.exe /link "/LIBPATH:%~2\bin_x64\lib" avutil-lav.lib
if errorlevel 1 exit /b %errorlevel%

call cl /nologo /EHsc /std:c++17 /O2 /MT /DLAV_ENABLE_OPENJOC /DLAV_OPENJOC_TESTING ^
  "/I%~2\decoder\LAVAudio" "/I%~2\include" "/I%~2\common\includes" ^
  "/I%~2\common\baseclasses" "/I%~2\ffmpeg" "/I%~2\libbluray\src" ^
  "/I%~2\common\DSUtilLite" "/I%~3" ^
  "%~2\decoder\LAVAudio\OpenJocDecoderSmoke.cpp" ^
  "%~2\decoder\LAVAudio\OpenJocDecoder.cpp" ^
  "%~2\decoder\LAVAudio\OpenJocAdmission.cpp" ^
  "%~2\decoder\LAVAudio\OpenJocOutput.cpp" ^
  /Fe:OpenJocDecoderSmoke.exe /link "/LIBPATH:%~2\bin_x64\lib" avutil-lav.lib
if errorlevel 1 exit /b %errorlevel%

call cl /nologo /EHsc /std:c++17 /O2 /MT ^
  "%~2\decoder\LAVAudio\LAVAudioIdentitySmoke.cpp" ^
  /Fe:LAVAudioIdentitySmoke.exe /link ole32.lib strmiids.lib
if errorlevel 1 exit /b %errorlevel%

call cl /nologo /EHsc /std:c++17 /O2 /MT "/I%~2\include" ^
  "%~4" /Fe:OpenJocDirectShowLifecycle.exe /link ole32.lib strmiids.lib
exit /b %errorlevel%
