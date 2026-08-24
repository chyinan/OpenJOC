@rem SPDX-FileCopyrightText: 2026 OpenJOC contributors
@rem SPDX-License-Identifier: GPL-2.0-or-later
@rem pattern: Imperative Shell
@echo off
setlocal EnableExtensions

if not "%~8"=="" goto :usage
if "%~7"=="" goto :usage

set "VSDEVCMD=%~f1"
set "TARGET_LAV_ROOT=%~f2"
set "PRISTINE_LAV_ROOT=%~f3"
set "OPENJOC_INCLUDE=%~f4"
set "OPENJOC_CAPI=%~f5"
set "FIXTURE_DIR=%~f6"
set "OUTPUT_DIR=%~f7"
set "TARGET_BUILD_DIR=%OUTPUT_DIR%\target-build"
set "PRISTINE_BUILD_DIR=%OUTPUT_DIR%\pristine-build"
set "TARGET_RUNTIME_DIR=%OUTPUT_DIR%\target-runtime"
set "PRISTINE_RUNTIME_DIR=%OUTPUT_DIR%\pristine-runtime"
set "HARNESS_OUTPUT_DIR=%OUTPUT_DIR%\harness"
set "TARGET_TASK3_EVIDENCE=%OUTPUT_DIR%\target-task3-evidence"
set "PRISTINE_TASK3_EVIDENCE=%OUTPUT_DIR%\pristine-task3-evidence"
set "PRISTINE_PROVENANCE=%PRISTINE_LAV_ROOT%\OPENJOC_PRISTINE_ARCHIVE_PROVENANCE.txt"
set "TARGET_RUNTIME_MANIFEST=%TARGET_RUNTIME_DIR%\OpenJocRuntimeIdentity.tsv"
set "PRISTINE_RUNTIME_MANIFEST=%PRISTINE_RUNTIME_DIR%\OpenJocRuntimeIdentity.tsv"
set "EXPECTED_PRISTINE_HEAD=b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27"
set "EXPECTED_PRISTINE_TREE=b39333900119799887bd84f21510d2179906826b"
set "EXPECTED_FFMPEG_ARCHIVE=5C24633B1DC5DD18AA07529AD73CDBCE9BB10F55AA3E39AA17027AB85C114B0E"
set "EXPECTED_LIBBLURAY_ARCHIVE=77824565B23684D5FE3DA7EA7A5081D58C89AF11DD7B01DB769A2765EE1F7C7A"
set "EXPECTED_QSDECODER_ARCHIVE=CDBD55F80C06F3C7E44C261DB47ECFBAC2B0A2EB5BC4C2696D00397F6E941D12"
set "EXPECTED_LIBUDFREAD_ARCHIVE=420A3962D283B23D10BA486E7A3AF2FC57C46C1E22116FF5AF6DF935651A6B89"

if /i "%TARGET_LAV_ROOT%"=="%PRISTINE_LAV_ROOT%" (
  >&2 echo target and pristine LAV roots must be distinct
  exit /b 65
)
if /i "%TARGET_BUILD_DIR%"=="%PRISTINE_BUILD_DIR%" (
  >&2 echo target and pristine build directories must be distinct
  exit /b 65
)
if /i "%TARGET_RUNTIME_DIR%"=="%PRISTINE_RUNTIME_DIR%" (
  >&2 echo target and pristine runtime directories must be distinct
  exit /b 65
)

for %%P in ("%VSDEVCMD%" "%TARGET_LAV_ROOT%\LAVFilters.sln" "%PRISTINE_LAV_ROOT%\LAVFilters.sln" "%OPENJOC_INCLUDE%" "%OPENJOC_CAPI%" "%FIXTURE_DIR%\joc.fingerprint.ec3" "%FIXTURE_DIR%\joc.fingerprint.mp4" "%FIXTURE_DIR%\joc.multi.ec3" "%FIXTURE_DIR%\joc.multi.mp4" "%FIXTURE_DIR%\joc.lifecycle.ec3" "%FIXTURE_DIR%\joc.lifecycle.mp4" "%FIXTURE_DIR%\ordinary.fingerprint.eac3" "%FIXTURE_DIR%\ordinary.fingerprint.mp4") do (
  if not exist "%%~fP" (
    >&2 echo required input does not exist: %%~fP
    exit /b 66
  )
)

set "PROVENANCE_GIT="
for /f "delims=" %%G in ('where.exe git.exe 2^>nul') do if not defined PROVENANCE_GIT set "PROVENANCE_GIT=%%~fG"
if not defined PROVENANCE_GIT (
  >&2 echo a real git.exe is required for frozen pristine provenance
  exit /b 67
)
set "TASK3_FFMPEG="
for /f "delims=" %%F in ('where.exe ffmpeg.exe 2^>nul') do if not defined TASK3_FFMPEG set "TASK3_FFMPEG=%%~fF"
if not defined TASK3_FFMPEG (
  >&2 echo a real ffmpeg.exe is required for Task3 fixture payload prequalification
  exit /b 67
)
set "TASK3_FFPROBE="
for /f "delims=" %%F in ('where.exe ffprobe.exe 2^>nul') do if not defined TASK3_FFPROBE set "TASK3_FFPROBE=%%~fF"
if not defined TASK3_FFPROBE (
  >&2 echo a real ffprobe.exe is required for Task3 fixture timing prequalification
  exit /b 67
)
set "PRISTINE_HEAD="
for /f "delims=" %%H in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" rev-parse HEAD') do set "PRISTINE_HEAD=%%H"
if errorlevel 1 exit /b 67
set "PRISTINE_TREE="
for /f "delims=" %%H in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" rev-parse HEAD:') do set "PRISTINE_TREE=%%H"
if errorlevel 1 exit /b 67
if /i not "%PRISTINE_HEAD%"=="%EXPECTED_PRISTINE_HEAD%" (
  >&2 echo pristine HEAD mismatch: expected %EXPECTED_PRISTINE_HEAD%, got %PRISTINE_HEAD%
  exit /b 67
)
if /i not "%PRISTINE_TREE%"=="%EXPECTED_PRISTINE_TREE%" (
  >&2 echo pristine tree mismatch: expected %EXPECTED_PRISTINE_TREE%, got %PRISTINE_TREE%
  exit /b 67
)

for %%S in (ffmpeg libbluray qsdecoder) do (
  if not exist "%PRISTINE_LAV_ROOT%\%%S" (
    >&2 echo pristine locked submodule source is missing: %%S
    exit /b 68
  )
)
for /f "tokens=3" %%H in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" ls-tree HEAD ffmpeg') do set "PRISTINE_FFMPEG_GITLINK=%%H"
for /f "tokens=3" %%H in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" ls-tree HEAD libbluray') do set "PRISTINE_LIBBLURAY_GITLINK=%%H"
for /f "tokens=3" %%H in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" ls-tree HEAD qsdecoder') do set "PRISTINE_QSDECODER_GITLINK=%%H"
if /i not "%PRISTINE_FFMPEG_GITLINK%"=="599d3a140460e1b57c234fe064db5185fb76ee5b" exit /b 68
if /i not "%PRISTINE_LIBBLURAY_GITLINK%"=="2df828e7dfef1d8c3fe7ebc2e8b764064a3f69f3" exit /b 68
if /i not "%PRISTINE_QSDECODER_GITLINK%"=="72e6b6a944460d3cbeffe13e78b88dd773a85602" exit /b 68
if not exist "%PRISTINE_PROVENANCE%" (
  >&2 echo pristine archive provenance sidecar is missing: %PRISTINE_PROVENANCE%
  exit /b 68
)
call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" diff --quiet --ignore-submodules=dirty --
if errorlevel 1 (
  >&2 echo pristine superproject tracked worktree is not frozen
  exit /b 68
)
call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" diff --cached --quiet --ignore-submodules=dirty HEAD --
if errorlevel 1 (
  >&2 echo pristine superproject index is not frozen
  exit /b 68
)
set "PRISTINE_UNTRACKED_COUNT=0"
set "PRISTINE_UNTRACKED_LINE="
for /f "delims=" %%S in ('call "%PROVENANCE_GIT%" -C "%PRISTINE_LAV_ROOT%" ls-files --others --exclude-standard') do (
  set /a PRISTINE_UNTRACKED_COUNT+=1
  set "PRISTINE_UNTRACKED_LINE=%%S"
)
if not "%PRISTINE_UNTRACKED_COUNT%"=="1" (
  >&2 echo pristine superproject untracked-file set is not frozen
  exit /b 68
)
if /i not "%PRISTINE_UNTRACKED_LINE%"=="OPENJOC_PRISTINE_ARCHIVE_PROVENANCE.txt" (
  >&2 echo unexpected pristine untracked file: %PRISTINE_UNTRACKED_LINE%
  exit /b 68
)
set "PROVENANCE_FFMPEG="
set "PROVENANCE_LIBBLURAY="
set "PROVENANCE_QSDECODER="
set "PROVENANCE_LIBUDFREAD="
set "PROVENANCE_FFMPEG_COUNT=0"
set "PROVENANCE_LIBBLURAY_COUNT=0"
set "PROVENANCE_QSDECODER_COUNT=0"
set "PROVENANCE_LIBUDFREAD_COUNT=0"
set "PROVENANCE_TOTAL_COUNT=0"
for /f "usebackq tokens=1,2,*" %%A in ("%PRISTINE_PROVENANCE%") do (
  if not "%%C"=="" exit /b 68
  set /a PROVENANCE_TOTAL_COUNT+=1
  if /i "%%A"=="ffmpeg" (
    set "PROVENANCE_FFMPEG=%%B"
    set /a PROVENANCE_FFMPEG_COUNT+=1
  ) else if /i "%%A"=="libbluray" (
    set "PROVENANCE_LIBBLURAY=%%B"
    set /a PROVENANCE_LIBBLURAY_COUNT+=1
  ) else if /i "%%A"=="qsdecoder" (
    set "PROVENANCE_QSDECODER=%%B"
    set /a PROVENANCE_QSDECODER_COUNT+=1
  ) else if /i "%%A"=="libudfread" (
    set "PROVENANCE_LIBUDFREAD=%%B"
    set /a PROVENANCE_LIBUDFREAD_COUNT+=1
  ) else (
    >&2 echo unknown pristine archive provenance record: %%A
    exit /b 68
  )
)
if not "%PROVENANCE_TOTAL_COUNT%"=="4" exit /b 68
if not "%PROVENANCE_FFMPEG_COUNT%"=="1" exit /b 68
if not "%PROVENANCE_LIBBLURAY_COUNT%"=="1" exit /b 68
if not "%PROVENANCE_QSDECODER_COUNT%"=="1" exit /b 68
if not "%PROVENANCE_LIBUDFREAD_COUNT%"=="1" exit /b 68
if /i not "%PROVENANCE_FFMPEG%"=="%EXPECTED_FFMPEG_ARCHIVE%" exit /b 68
if /i not "%PROVENANCE_LIBBLURAY%"=="%EXPECTED_LIBBLURAY_ARCHIVE%" exit /b 68
if /i not "%PROVENANCE_QSDECODER%"=="%EXPECTED_QSDECODER_ARCHIVE%" exit /b 68
if /i not "%PROVENANCE_LIBUDFREAD%"=="%EXPECTED_LIBUDFREAD_ARCHIVE%" exit /b 68

if exist "%OUTPUT_DIR%" (
  >&2 echo refusing to reuse output directory: %OUTPUT_DIR%
  exit /b 69
)
mkdir "%OUTPUT_DIR%"
mkdir "%TARGET_BUILD_DIR%" "%PRISTINE_BUILD_DIR%" "%TARGET_RUNTIME_DIR%" "%PRISTINE_RUNTIME_DIR%" "%HARNESS_OUTPUT_DIR%" "%TARGET_TASK3_EVIDENCE%" "%PRISTINE_TASK3_EVIDENCE%"
if errorlevel 1 exit /b 1
set "TASK3_ORDINARY_DEMUX=%OUTPUT_DIR%\ordinary.fingerprint.demux.eac3"
call "%TASK3_FFMPEG%" -v error -i "%FIXTURE_DIR%\ordinary.fingerprint.mp4" -map 0:a:0 -c:a copy -f eac3 -y "%TASK3_ORDINARY_DEMUX%"
if errorlevel 1 exit /b 1
fc.exe /b "%FIXTURE_DIR%\ordinary.fingerprint.eac3" "%TASK3_ORDINARY_DEMUX%" >nul
if errorlevel 1 (
  >&2 echo ordinary raw and MP4 E-AC-3 payloads differ
  exit /b 70
)
echo TASK3_FIXTURE_PREQUAL ordinary_raw="%FIXTURE_DIR%\ordinary.fingerprint.eac3" ordinary_mp4="%FIXTURE_DIR%\ordinary.fingerprint.mp4" demuxed="%TASK3_ORDINARY_DEMUX%"
certutil.exe -hashfile "%FIXTURE_DIR%\ordinary.fingerprint.eac3" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%FIXTURE_DIR%\ordinary.fingerprint.mp4" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%TASK3_ORDINARY_DEMUX%" SHA256
if errorlevel 1 exit /b 1
attrib +R "%TASK3_ORDINARY_DEMUX%"
if errorlevel 1 exit /b 1
set "TASK3_JOC_DEMUX=%OUTPUT_DIR%\joc.multi.demux.ec3"
call "%TASK3_FFMPEG%" -v error -i "%FIXTURE_DIR%\joc.multi.mp4" -map 0:a:0 -c:a copy -f eac3 -y "%TASK3_JOC_DEMUX%"
if errorlevel 1 exit /b 1
fc.exe /b "%FIXTURE_DIR%\joc.multi.ec3" "%TASK3_JOC_DEMUX%" >nul
if errorlevel 1 (
  >&2 echo JOC raw and MP4 E-AC-3 payloads differ
  exit /b 70
)
echo TASK3_FIXTURE_PREQUAL joc_raw="%FIXTURE_DIR%\joc.multi.ec3" joc_mp4="%FIXTURE_DIR%\joc.multi.mp4" demuxed="%TASK3_JOC_DEMUX%"
certutil.exe -hashfile "%FIXTURE_DIR%\joc.multi.ec3" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%FIXTURE_DIR%\joc.multi.mp4" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%TASK3_JOC_DEMUX%" SHA256
if errorlevel 1 exit /b 1
attrib +R "%TASK3_JOC_DEMUX%"
if errorlevel 1 exit /b 1
set "TASK3_LIFECYCLE_DEMUX=%OUTPUT_DIR%\joc.lifecycle.demux.ec3"
set "TASK3_LIFECYCLE_STREAM_TIMING=%OUTPUT_DIR%\joc.lifecycle.stream-timing.txt"
set "TASK3_LIFECYCLE_PACKET_PTS=%OUTPUT_DIR%\joc.lifecycle.packet-pts.txt"
set "TASK3_LIFECYCLE_FRAME_TIMING=%OUTPUT_DIR%\joc.lifecycle.frame-timing.csv"
call "%TASK3_FFMPEG%" -v error -i "%FIXTURE_DIR%\joc.lifecycle.mp4" -map 0:a:0 -c:a copy -f eac3 -y "%TASK3_LIFECYCLE_DEMUX%"
if errorlevel 1 exit /b 1
fc.exe /b "%FIXTURE_DIR%\joc.lifecycle.ec3" "%TASK3_LIFECYCLE_DEMUX%" >nul
if errorlevel 1 (
  >&2 echo lifecycle raw and MP4 E-AC-3 payloads differ
  exit /b 70
)
call "%TASK3_FFPROBE%" -v error -select_streams a:0 -count_packets -show_entries stream=time_base,duration_ts,duration,nb_frames,nb_read_packets -of default=noprint_wrappers=1 "%FIXTURE_DIR%\joc.lifecycle.mp4" >"%TASK3_LIFECYCLE_STREAM_TIMING%"
if errorlevel 1 exit /b 1
call "%TASK3_FFPROBE%" -v error -select_streams a:0 -show_packets -show_entries packet=pts,dts -of csv=p=0 "%FIXTURE_DIR%\joc.lifecycle.mp4" >"%TASK3_LIFECYCLE_PACKET_PTS%"
if errorlevel 1 exit /b 1
call "%TASK3_FFPROBE%" -v error -select_streams a:0 -show_frames -show_entries frame=pts,pkt_dts,duration,nb_samples:frame_side_data= -of csv=p=0 "%FIXTURE_DIR%\joc.lifecycle.mp4" >"%TASK3_LIFECYCLE_FRAME_TIMING%"
if errorlevel 1 exit /b 1
powershell.exe -NoLogo -NoProfile -NonInteractive -Command "& { $stream = @(Get-Content -LiteralPath $args[0]); if ($stream.Count -ne 5 -or @($stream | Where-Object { $_ -ceq 'time_base=1/48000' }).Count -ne 1 -or @($stream | Where-Object { $_ -ceq 'duration_ts=196608' }).Count -ne 1 -or @($stream | Where-Object { $_ -ceq 'duration=4.096000' }).Count -ne 1 -or @($stream | Where-Object { $_ -ceq 'nb_frames=128' }).Count -ne 1 -or @($stream | Where-Object { $_ -ceq 'nb_read_packets=128' }).Count -ne 1) { exit 71 }; $packets = @(Import-Csv -LiteralPath $args[1] -Header pts,dts); if ($packets.Count -ne 128) { exit 71 }; for ($i = 0; $i -lt $packets.Count; ++$i) { $expected = [string]($i * 1536); if ($packets[$i].pts -cne $expected -or $packets[$i].dts -cne $expected) { exit 71 } }; $frames = @(Import-Csv -LiteralPath $args[2] -Header pts,pkt_dts,duration,nb_samples,empty); if ($frames.Count -ne 128) { exit 71 }; for ($i = 0; $i -lt $frames.Count; ++$i) { $expected = [string]($i * 1536); if ($frames[$i].pts -cne $expected -or $frames[$i].pkt_dts -cne $expected -or $frames[$i].duration -cne 'N/A' -or $frames[$i].nb_samples -cne '1536' -or $frames[$i].empty) { exit 71 } } }" "%TASK3_LIFECYCLE_STREAM_TIMING%" "%TASK3_LIFECYCLE_PACKET_PTS%" "%TASK3_LIFECYCLE_FRAME_TIMING%"
if errorlevel 1 (
  >&2 echo lifecycle MP4 timing prequalification failed
  exit /b 71
)
echo TASK3_LIFECYCLE_TIMING_PREQUAL raw="%FIXTURE_DIR%\joc.lifecycle.ec3" mp4="%FIXTURE_DIR%\joc.lifecycle.mp4" demuxed="%TASK3_LIFECYCLE_DEMUX%" time_base=1/48000 duration_ts=196608 duration=4.096000 nb_frames=128 nb_read_packets=128 packet_pts_dts_step=1536 frame_pts_dts_step=1536 frame_samples=1536 frame_duration=N/A
certutil.exe -hashfile "%FIXTURE_DIR%\joc.lifecycle.ec3" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%FIXTURE_DIR%\joc.lifecycle.mp4" SHA256
if errorlevel 1 exit /b 1
certutil.exe -hashfile "%TASK3_LIFECYCLE_DEMUX%" SHA256
if errorlevel 1 exit /b 1
for %%P in ("%TASK3_LIFECYCLE_DEMUX%" "%TASK3_LIFECYCLE_STREAM_TIMING%" "%TASK3_LIFECYCLE_PACKET_PTS%" "%TASK3_LIFECYCLE_FRAME_TIMING%") do (
  attrib +R "%%~fP"
  if errorlevel 1 exit /b 1
)
set "NOGIT_DIR=%OUTPUT_DIR%\no-git"
set "SERIAL_BUILD_PROPS=%OUTPUT_DIR%\OpenJocSerialEvidenceBuild.props"
mkdir "%NOGIT_DIR%"
if errorlevel 1 exit /b 1
>"%NOGIT_DIR%\git.cmd" echo @exit /b 1
if errorlevel 1 exit /b 1
>"%SERIAL_BUILD_PROPS%" echo ^<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003"^>
>>"%SERIAL_BUILD_PROPS%" echo   ^<PropertyGroup^>
>>"%SERIAL_BUILD_PROPS%" echo     ^<IntDir^>$(OpenJocEvidenceIntermediateRoot)\$(MSBuildProjectName)\^</IntDir^>
>>"%SERIAL_BUILD_PROPS%" echo   ^</PropertyGroup^>
>>"%SERIAL_BUILD_PROPS%" echo   ^<ItemDefinitionGroup^>
>>"%SERIAL_BUILD_PROPS%" echo     ^<ClCompile^>
>>"%SERIAL_BUILD_PROPS%" echo       ^<MultiProcessorCompilation^>false^</MultiProcessorCompilation^>
>>"%SERIAL_BUILD_PROPS%" echo     ^</ClCompile^>
>>"%SERIAL_BUILD_PROPS%" echo   ^</ItemDefinitionGroup^>
>>"%SERIAL_BUILD_PROPS%" echo ^</Project^>
if errorlevel 1 exit /b 1
set "PATH=%NOGIT_DIR%;%PATH%"

call "%VSDEVCMD%" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b 1

call :build_lane "%TARGET_LAV_ROOT%" "%TARGET_BUILD_DIR%" true
if errorlevel 1 exit /b 1

call :build_lane "%PRISTINE_LAV_ROOT%" "%PRISTINE_BUILD_DIR%" false
if errorlevel 1 exit /b 1

for %%F in (LAVAudio.ax LAVSplitter.ax libbluray.dll) do (
  if not exist "%TARGET_BUILD_DIR%\%%F" (
    >&2 echo target build artifact missing: %%F
    exit /b 70
  )
  if not exist "%PRISTINE_BUILD_DIR%\%%F" (
    >&2 echo pristine build artifact missing: %%F
    exit /b 70
  )
  copy /y "%TARGET_BUILD_DIR%\%%F" "%TARGET_RUNTIME_DIR%\%%F" >nul
  if errorlevel 1 exit /b 1
  copy /y "%PRISTINE_BUILD_DIR%\%%F" "%PRISTINE_RUNTIME_DIR%\%%F" >nul
  if errorlevel 1 exit /b 1
)
copy /y "%OPENJOC_CAPI%" "%TARGET_RUNTIME_DIR%\openjoc_capi.dll" >nul
if errorlevel 1 exit /b 1
copy /y "%OPENJOC_CAPI%" "%PRISTINE_RUNTIME_DIR%\openjoc_capi.dll" >nul
if errorlevel 1 exit /b 1
copy /y "%TARGET_LAV_ROOT%\resources\LAVFilters.Dependencies.manifest" ^
  "%TARGET_RUNTIME_DIR%\LAVFilters.Dependencies.manifest" >nul
if errorlevel 1 exit /b 1
copy /y "%PRISTINE_LAV_ROOT%\resources\LAVFilters.Dependencies.manifest" ^
  "%PRISTINE_RUNTIME_DIR%\LAVFilters.Dependencies.manifest" >nul
if errorlevel 1 exit /b 1

set "TARGET_FFMPEG_COUNT=0"
for %%F in ("%TARGET_LAV_ROOT%\bin_x64\*-lav-*.dll") do (
  if exist "%%~fF" (
    copy /y "%%~fF" "%TARGET_RUNTIME_DIR%\%%~nxF" >nul
    if errorlevel 1 exit /b 1
    set /a TARGET_FFMPEG_COUNT+=1
  )
)
set "PRISTINE_FFMPEG_COUNT=0"
for %%F in ("%PRISTINE_LAV_ROOT%\bin_x64\*-lav-*.dll") do (
  if exist "%%~fF" (
    copy /y "%%~fF" "%PRISTINE_RUNTIME_DIR%\%%~nxF" >nul
    if errorlevel 1 exit /b 1
    set /a PRISTINE_FFMPEG_COUNT+=1
  )
)
if "%TARGET_FFMPEG_COUNT%"=="0" (
  >&2 echo target FFmpeg runtime DLLs are missing
  exit /b 70
)
if "%PRISTINE_FFMPEG_COUNT%"=="0" (
  >&2 echo pristine FFmpeg runtime DLLs are missing
  exit /b 70
)

pushd "%HARNESS_OUTPUT_DIR%"
if errorlevel 1 exit /b 1
call cl /nologo /EHsc /std:c++17 /O2 /MT /utf-8 /DPSAPI_VERSION=2 ^
  /DUNICODE /D_UNICODE /DLAV_ENABLE_OPENJOC ^
  "/I%TARGET_LAV_ROOT%\decoder\LAVAudio" ^
  "/I%TARGET_LAV_ROOT%\include" ^
  "/I%TARGET_LAV_ROOT%\common\includes" ^
  "/I%TARGET_LAV_ROOT%\common\baseclasses" ^
  "/I%TARGET_LAV_ROOT%\common\DSUtilLite" ^
  "/I%TARGET_LAV_ROOT%\ffmpeg" ^
  "/I%OPENJOC_INCLUDE%" ^
  "%TARGET_LAV_ROOT%\decoder\LAVAudio\OpenJocDirectShowNegotiationSmoke.cpp" ^
  "%TARGET_LAV_ROOT%\decoder\LAVAudio\OpenJocDecoder.cpp" ^
  "%TARGET_LAV_ROOT%\decoder\LAVAudio\OpenJocAdmission.cpp" ^
  "%TARGET_LAV_ROOT%\decoder\LAVAudio\OpenJocOutput.cpp" ^
  "%TARGET_LAV_ROOT%\decoder\LAVAudio\OpenJocStrictOutput.cpp" ^
  /Fe:OpenJocDirectShowNegotiationSmoke.exe ^
  /link "/LIBPATH:%TARGET_BUILD_DIR%" "/LIBPATH:%TARGET_LAV_ROOT%\bin_x64\lib" ^
  strmbase.lib strmiids.lib ole32.lib uuid.lib user32.lib advapi32.lib winmm.lib bcrypt.lib avutil-lav.lib
if errorlevel 1 exit /b 1

copy /y "%HARNESS_OUTPUT_DIR%\OpenJocDirectShowNegotiationSmoke.exe" ^
  "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" >nul
if errorlevel 1 exit /b 1
copy /y "%HARNESS_OUTPUT_DIR%\OpenJocDirectShowNegotiationSmoke.exe" ^
  "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" >nul
if errorlevel 1 exit /b 1

call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --write-manifest "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%"
if errorlevel 1 exit /b 1
attrib +R "%TARGET_RUNTIME_MANIFEST%"
if errorlevel 1 exit /b 1
call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --write-manifest "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%"
if errorlevel 1 exit /b 1
attrib +R "%PRISTINE_RUNTIME_MANIFEST%"
if errorlevel 1 exit /b 1

call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --self-test "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" target
if errorlevel 1 exit /b 1
call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --self-test "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%" pristine
if errorlevel 1 exit /b 1
call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --controlled-sink "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" "%FIXTURE_DIR%"
if errorlevel 1 exit /b 1

call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --stock-eac3-worker "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%" pristine "%FIXTURE_DIR%\ordinary.fingerprint.eac3" "%PRISTINE_TASK3_EVIDENCE%\stock-eac3.tsv" 0
if errorlevel 1 exit /b 1
attrib +R "%PRISTINE_TASK3_EVIDENCE%\stock-eac3.tsv"
if errorlevel 1 exit /b 1
call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --eac3-passthrough-worker "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%" pristine "%FIXTURE_DIR%\joc.multi.ec3" "%PRISTINE_TASK3_EVIDENCE%\passthrough-ec3.tsv" 0
if errorlevel 1 exit /b 1
attrib +R "%PRISTINE_TASK3_EVIDENCE%\passthrough-ec3.tsv"
if errorlevel 1 exit /b 1
for %%P in (0 1 2 3 4 5 6) do (
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --stock-eac3-worker "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" target "%FIXTURE_DIR%\ordinary.fingerprint.eac3" "%TARGET_TASK3_EVIDENCE%\stock-eac3-%%P.tsv" %%P
  if errorlevel 1 exit /b 1
  attrib +R "%TARGET_TASK3_EVIDENCE%\stock-eac3-%%P.tsv"
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --compare-task3-evidence "%TARGET_TASK3_EVIDENCE%\stock-eac3-%%P.tsv" "%PRISTINE_TASK3_EVIDENCE%\stock-eac3.tsv" %%P stock
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --eac3-passthrough-worker "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" target "%FIXTURE_DIR%\joc.multi.ec3" "%TARGET_TASK3_EVIDENCE%\passthrough-ec3-%%P.tsv" %%P
  if errorlevel 1 exit /b 1
  attrib +R "%TARGET_TASK3_EVIDENCE%\passthrough-ec3-%%P.tsv"
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --compare-task3-evidence "%TARGET_TASK3_EVIDENCE%\passthrough-ec3-%%P.tsv" "%PRISTINE_TASK3_EVIDENCE%\passthrough-ec3.tsv" %%P passthrough
  if errorlevel 1 exit /b 1
)
call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --stock-eac3-worker "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%" pristine "%FIXTURE_DIR%\ordinary.fingerprint.mp4" "%PRISTINE_TASK3_EVIDENCE%\stock-mp4.tsv" 0
if errorlevel 1 exit /b 1
attrib +R "%PRISTINE_TASK3_EVIDENCE%\stock-mp4.tsv"
if errorlevel 1 exit /b 1
call "%PRISTINE_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --eac3-passthrough-worker "%PRISTINE_RUNTIME_DIR%" "%PRISTINE_RUNTIME_MANIFEST%" pristine "%FIXTURE_DIR%\joc.multi.mp4" "%PRISTINE_TASK3_EVIDENCE%\passthrough-mp4.tsv" 0
if errorlevel 1 exit /b 1
attrib +R "%PRISTINE_TASK3_EVIDENCE%\passthrough-mp4.tsv"
if errorlevel 1 exit /b 1
for %%P in (0 1 2 3 4 5 6) do (
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --stock-eac3-worker "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" target "%FIXTURE_DIR%\ordinary.fingerprint.mp4" "%TARGET_TASK3_EVIDENCE%\stock-mp4-%%P.tsv" %%P
  if errorlevel 1 exit /b 1
  attrib +R "%TARGET_TASK3_EVIDENCE%\stock-mp4-%%P.tsv"
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --compare-task3-evidence "%TARGET_TASK3_EVIDENCE%\stock-mp4-%%P.tsv" "%PRISTINE_TASK3_EVIDENCE%\stock-mp4.tsv" %%P stock
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --eac3-passthrough-worker "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" target "%FIXTURE_DIR%\joc.multi.mp4" "%TARGET_TASK3_EVIDENCE%\passthrough-mp4-%%P.tsv" %%P
  if errorlevel 1 exit /b 1
  attrib +R "%TARGET_TASK3_EVIDENCE%\passthrough-mp4-%%P.tsv"
  if errorlevel 1 exit /b 1
  call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --compare-task3-evidence "%TARGET_TASK3_EVIDENCE%\passthrough-mp4-%%P.tsv" "%PRISTINE_TASK3_EVIDENCE%\passthrough-mp4.tsv" %%P passthrough
  if errorlevel 1 exit /b 1
)

call "%TARGET_RUNTIME_DIR%\OpenJocDirectShowNegotiationSmoke.exe" --openjoc-lifecycle "%TARGET_RUNTIME_DIR%" "%TARGET_RUNTIME_MANIFEST%" "%FIXTURE_DIR%"
exit /b %errorlevel%

:build_lane
for %%T in (baseclasses DSUtilLite libbluray Demuxers LAVAudio LAVSplitter) do (
  call msbuild "%~1\LAVFilters.sln" "/t:%%T:Rebuild" /nologo /v:minimal ^
    /p:Configuration=Release /p:Platform=x64 /p:BuildProjectReferences=false ^
    /p:CL_MPCount=1 /p:UseMultiToolTask=true /p:MultiProcMaxCount=1 ^
    "/p:ForceImportBeforeCppTargets=%SERIAL_BUILD_PROPS%" ^
    /p:EnableOpenJOC=true /p:EnableOpenJOCSideBySide=%~3 ^
    "/p:OpenJocIncludeDir=%OPENJOC_INCLUDE%" ^
    "/p:OutDir=%~2/" "/p:OpenJocEvidenceIntermediateRoot=%~2\obj"
  if errorlevel 1 exit /b 1
)
exit /b 0

:usage
>&2 echo Usage: test_lav_directshow_negotiation.cmd VSDEVCMD TARGET_LAV_ROOT PRISTINE_LAV_ROOT OPENJOC_INCLUDE OPENJOC_CAPI FIXTURE_DIR OUTPUT_DIR
exit /b 64
