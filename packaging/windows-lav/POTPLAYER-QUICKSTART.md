# PotPlayer quick start (30-60 seconds)

OpenJOC LAV must already show **PASS** when you double-click `verify.bat`.

1. Close and reopen PotPlayer if it was running during installation.
2. Open **Preferences** (`F5`).
3. Select **Filter Control**, then **Filter Priority (Overall)**.
4. Choose **Add registered filter**.
5. Select **LAV Audio Decoder (OpenJOC)** and choose **Add**.
6. Set its priority to **Prefer**, then select **Apply** and **OK**.
7. Play your JOC file.

Do not remove or lower the priority of your stock LAV decoder. OpenJOC uses a
separate filter identity. PotPlayer wording can vary slightly by version; if
the OpenJOC filter is not listed, run `verify.bat`, then run `install.bat`
again if verification reports a failure.
