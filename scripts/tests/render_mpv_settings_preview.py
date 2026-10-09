#!/usr/bin/env python3
"""Render the mocked mpv settings-panel ASS overlay with FFmpeg/libass.

This checks panel layout and clipping at a 16:9 desktop size and a compact
window size. It is a rendering preview, not a test of mpv's actual input or
window-system event delivery.
"""

from __future__ import annotations

import argparse
import os
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LUA_TEST = ROOT / "integrations/mpv/test-openjoc-settings.lua"


def lua_command() -> list[str]:
    if executable := shutil.which("luajit"):
        return [executable, str(LUA_TEST)]
    if executable := shutil.which("lua"):
        return [executable, str(LUA_TEST)]
    if executable := shutil.which("texlua"):
        return [executable, "--luaonly", str(LUA_TEST)]
    raise RuntimeError("Install LuaJIT (or Lua/texlua) to run the panel interaction test")


def ass_document(events: str, width: int, height: int) -> str:
    dialogue_lines = "\n".join(
        f"Dialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,{line}"
        for line in events.splitlines()
        if line
    )
    return f"""[Script Info]
ScriptType: v4.00+
WrapStyle: 2
ScaledBorderAndShadow: yes
PlayResX: {width}
PlayResY: {height}

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,18,&H00F4F7F8,&H000000FF,&H00000000,&H64000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
{dialogue_lines}
"""


def png_dimensions(path: Path) -> tuple[int, int]:
    with path.open("rb") as image:
        header = image.read(24)
    if header[:8] != b"\x89PNG\r\n\x1a\n":
        raise RuntimeError(f"FFmpeg did not produce a PNG: {path}")
    return struct.unpack(">II", header[16:24])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "build/mpv-settings-preview",
        help="Directory for rendered PNG previews (default: build/mpv-settings-preview)",
    )
    args = parser.parse_args()
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise RuntimeError("FFmpeg with the libass filter is required for the visual preview")
    filters = subprocess.run(
        [ffmpeg, "-hide_banner", "-filters"], capture_output=True, text=True, check=True
    ).stdout
    if not any(line.split()[:2] == ["...", "ass"] for line in filters.splitlines()):
        raise RuntimeError("This FFmpeg build does not include the libass `ass` filter")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="openjoc-mpv-ass-") as temp_name:
        temp = Path(temp_name)
        cache = temp / "fontconfig-cache"
        cache.mkdir()
        env = os.environ.copy()
        env["OPENJOC_ASS_PREVIEW_DIR"] = str(temp)
        env["HOME"] = str(temp)
        env["XDG_CACHE_HOME"] = str(cache)
        completed = subprocess.run(lua_command(), cwd=ROOT, env=env, check=False)
        if completed.returncode:
            return completed.returncode

        for name, width, height in (
            ("openjoc-settings-1280x720", 1280, 720),
            ("openjoc-settings-640x480", 640, 480),
        ):
            capture = temp / f"{name}.ass-events"
            if not capture.is_file() or not capture.read_text(encoding="utf-8"):
                raise RuntimeError(f"The Lua interaction test did not capture {name}")
            subtitle = temp / f"{name}.ass"
            subtitle.write_text(
                ass_document(capture.read_text(encoding="utf-8"), width, height),
                encoding="utf-8",
            )
            output = (args.output_dir / f"{name}.png").resolve()
            subprocess.run(
                [
                    ffmpeg,
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    f"testsrc2=size={width}x{height}:duration=1:rate=1",
                    "-vf",
                    f"ass={subtitle.name}",
                    "-frames:v",
                    "1",
                    str(output),
                ],
                cwd=temp,
                env=env,
                check=True,
            )
            actual = png_dimensions(output)
            if actual != (width, height):
                raise RuntimeError(f"Unexpected preview dimensions for {name}: {actual}")
            print(f"Rendered {output} ({actual[0]}x{actual[1]})")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as exc:
        raise SystemExit(str(exc)) from exc
