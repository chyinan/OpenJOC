# PotPlayer quick start (30-60 seconds)

OpenJOC LAV must already show **PASS** when you double-click `verify.bat`.

**Check LAV Splitter separately.** A `verify.bat` **PASS** does not verify the
active splitter. This audio-only package does not update
LAV Splitter / LAV Splitter Source or LAV Video. An old splitter can
misidentify MP4/MOV PCM and produce loud static even with a current audio
decoder. Check the active splitter's version and file path, including any
manually added external filter path in PotPlayer. If static occurs, stop
playback and mute the output before inspecting or reopening the file. Follow
the [PCM troubleshooting guide](https://chyinan.github.io/OpenJOC/using/troubleshooting/#pcm-noise-with-an-old-lav-splitter)
and [splitter-only update instructions](https://chyinan.github.io/OpenJOC/using/windows-lav-potplayer/#update-only-lav-splitter).
Keep the complete official x64 LAV package in its own folder; do not mix its
DLLs with the OpenJOC runtime or replace the OpenJOC audio filter selection.

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
