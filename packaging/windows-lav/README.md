# OpenJOC LAV 0.16.0 for Windows x64

## PotPlayer users

1. Extract the ZIP.
2. Double-click `install.bat`.
3. Accept the Windows administrator prompt.
4. Wait for **INSTALLATION SUCCESSFUL**.
5. Follow `POTPLAYER-QUICKSTART.md`.
6. Play your JOC file.

Installation and PotPlayer filter selection are separate. Installing OpenJOC
LAV does not silently change PotPlayer, stock LAV, K-Lite, PATH, or PowerShell
execution policy.

Double-click `verify.bat` whenever you want to check the installation. To
remove only OpenJOC-owned files and registration, double-click
`uninstall.bat`.

## Automation

The PowerShell scripts in `scripts/` accept `-NonInteractive`. This disables
the final keypress and returns a stable process exit code. `install.ps1` and
`uninstall.ps1` request UAC automatically when required.

| Code | Meaning |
| 0 | Success, including an already-absent uninstall |
| 10 | User cancelled the UAC prompt or uninstall confirmation |
| 20 | Package, architecture, or prerequisite check failed |
| 30 | Installation or DirectShow registration failed |
| 40 | Post-install or standalone verification failed |
| 50 | Uninstall or rollback failed |

Logs are written to `%LOCALAPPDATA%\OpenJOC\Logs`. No environment dump,
credentials, tokens, or unrelated user data are recorded.

Advanced example:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -NonInteractive
```
