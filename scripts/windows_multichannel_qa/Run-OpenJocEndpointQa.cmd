@rem SPDX-FileCopyrightText: 2026 OpenJOC contributors
@rem SPDX-License-Identifier: Apache-2.0
@echo off
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-OpenJocEndpointQa.ps1" %*
exit /b %errorlevel%

