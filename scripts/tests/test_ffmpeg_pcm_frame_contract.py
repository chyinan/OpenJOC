"""Keep the FFmpeg bridge aligned with the OpenJOC PCM frame ABI."""

# pattern: Imperative Shell

from pathlib import Path
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PATCH_PATH = (
    ROOT
    / "integrations"
    / "ffmpeg"
    / "native"
    / "patches"
    / "0001-avcodec-add-experimental-libopenjoc-decoder-wrapper.patch"
)
HEADER_PATH = ROOT / "crates" / "openjoc-capi" / "include" / "openjoc.h"


def extract_added_function(patch: str, name: str) -> str:
    signature = re.escape(f"static int {name}(")
    match = re.search(
        rf"(?ms)^(\+{signature}.*?^\+}}\n)",
        patch,
    )
    if match is None:
        raise AssertionError(f"patched C function is missing: {name}")
    return "\n".join(line[1:] for line in match.group(1).splitlines()) + "\n"


def split_compiler_command(configured_cc: str, *, windows: bool) -> list[str]:
    if not windows:
        return shlex.split(configured_cc)

    value = configured_cc.strip()
    candidate = value[1:-1] if value.startswith('"') and value.endswith('"') else value
    resolved = shutil.which(candidate)
    if resolved:
        return [resolved]

    lexer = shlex.shlex(value, posix=False)
    lexer.whitespace_split = True
    tokens = list(lexer)
    return [
        token[1:-1]
        if len(token) >= 2 and token.startswith('"') and token.endswith('"')
        else token
        for token in tokens
    ]


class FfmpegPcmFrameContractTests(unittest.TestCase):
    def test_windows_compiler_environment_preserves_drive_path(self) -> None:
        compiler = r"C:\msys64\mingw64\bin\gcc.exe"
        self.assertEqual(
            split_compiler_command(compiler, windows=True),
            [compiler],
        )
        spaced_compiler = r'"C:\Program Files\MinGW\bin\gcc.exe" -O2'
        self.assertEqual(
            split_compiler_command(spaced_compiler, windows=True),
            [r"C:\Program Files\MinGW\bin\gcc.exe", "-O2"],
        )

    def test_new_file_hunk_length_matches_the_patch_body(self) -> None:
        patch = PATCH_PATH.read_text(encoding="utf-8")
        match = re.search(
            r"(?ms)^@@ -0,0 \+1,(\d+) @@\n(.*?)(?=^diff --git|\Z)", patch
        )

        self.assertIsNotNone(match, "new FFmpeg decoder source hunk must exist")
        assert match is not None
        declared_lines = int(match.group(1))
        added_lines = sum(
            line.startswith("+") and not line.startswith("+++")
            for line in match.group(2).splitlines()
        )
        self.assertEqual(declared_lines, added_lines)

    def test_ffmpeg_wrapper_validates_and_copies_pcm_payload_bytes(self) -> None:
        header = HEADER_PATH.read_text(encoding="utf-8")
        patch = PATCH_PATH.read_text(encoding="utf-8")
        match = re.search(
            r"\+static int libopenjoc_output_frame\(.*?\n\+}\n",
            patch,
            re.DOTALL,
        )

        self.assertIn("byte length of the interleaved float32 payload", header)
        self.assertIsNotNone(match, "patched FFmpeg PCM output adapter must exist")
        assert match is not None
        function = match.group(0)
        helper = extract_added_function(patch, "libopenjoc_pcm_payload_bytes")
        self.assertIn("sample_count > SIZE_MAX / channel_count", helper)
        self.assertIn("*expected_samples > SIZE_MAX / sizeof(float)", helper)
        self.assertIn("size_t expected_bytes;", function)
        self.assertIn("libopenjoc_pcm_payload_bytes(pcm->sample_count", function)
        self.assertIn(
            "*expected_bytes = *expected_samples * sizeof(float);", helper
        )
        self.assertNotIn("pcm->data_len != expected_samples", function)
        self.assertEqual(function.count("pcm->data_len !="), 1)
        self.assertIn("pcm->data_len != expected_bytes", function)
        self.assertLess(
            helper.index("*expected_bytes = *expected_samples * sizeof(float);"),
            function.index("pcm->data_len != expected_bytes"),
        )
        self.assertIn("memcpy(frame->data[0], pcm->data, expected_bytes);", function)

    def test_compiled_pcm_adapter_accepts_and_rejects_byte_lengths(self) -> None:
        configured_cc = os.environ.get("CC")
        compiler = (
            split_compiler_command(configured_cc, windows=os.name == "nt")
            if configured_cc
            else []
        )
        if not compiler:
            compiler_path = next(
                (shutil.which(name) for name in ("cc", "clang", "gcc") if shutil.which(name)),
                None,
            )
            if compiler_path is None:
                self.skipTest("a C compiler is required for the FFmpeg adapter contract test")
            compiler = [compiler_path]

        patch = PATCH_PATH.read_text(encoding="utf-8")
        helper = extract_added_function(patch, "libopenjoc_pcm_payload_bytes")
        adapter = extract_added_function(patch, "libopenjoc_output_frame")
        harness = r"""
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define AVERROR_INVALIDDATA (-22)
#define AVERROR(error) (-(error))
#define AV_SAMPLE_FMT_FLT 1
#define AV_LOG_ERROR 16
#define OPENJOC_NO_PTS INT64_MIN
#define AV_NOPTS_VALUE INT64_MIN

typedef struct { int num; int den; } AVRational;
typedef struct { int nb_channels; } AVChannelLayout;
typedef struct {
    AVRational pkt_timebase;
    AVChannelLayout ch_layout;
} AVCodecContext;
typedef struct {
    int format;
    int sample_rate;
    int nb_samples;
    AVChannelLayout ch_layout;
    uint8_t *data[1];
    int64_t pts;
    int64_t pkt_dts;
    AVRational time_base;
    int64_t duration;
} AVFrame;
typedef struct {
    uint32_t sample_format;
    uint32_t sample_rate;
    uint32_t channel_count;
    size_t sample_count;
    int64_t pts_samples;
    const float *data;
    size_t data_len;
} openjoc_pcm_frame;

static int av_channel_layout_copy(AVChannelLayout *destination,
                                  const AVChannelLayout *source)
{
    *destination = *source;
    return 0;
}

static int ff_get_buffer(AVCodecContext *context, AVFrame *frame, int flags)
{
    size_t bytes = (size_t)frame->nb_samples *
                   (size_t)frame->ch_layout.nb_channels * sizeof(float);
    (void)context;
    (void)flags;
    frame->data[0] = malloc(bytes);
    return frame->data[0] ? 0 : -12;
}

static int64_t av_rescale_q(int64_t value, AVRational source, AVRational target)
{
    (void)source;
    (void)target;
    return value;
}

static void av_log(AVCodecContext *context, int level, const char *format, ...)
{
    (void)context;
    (void)level;
    (void)format;
}
""" + helper + adapter + r"""
static int check_frame(size_t data_len, int expected_status)
{
    const float source[] = { 0.125f, -0.25f, 0.5f, -1.0f };
    AVCodecContext context = { 0 };
    AVFrame frame = { 0 };
    openjoc_pcm_frame pcm = {
        .sample_format = 1,
        .sample_rate = 48000,
        .channel_count = 2,
        .sample_count = 2,
        .pts_samples = OPENJOC_NO_PTS,
        .data = source,
        .data_len = data_len,
    };

    context.pkt_timebase.num = 1;
    context.pkt_timebase.den = 48000;
    context.ch_layout.nb_channels = 2;
    int status = libopenjoc_output_frame(&context, &frame, &pcm);
    if (status != expected_status)
        return 1;
    if (status == 0 &&
        (frame.nb_samples != 2 || frame.format != AV_SAMPLE_FMT_FLT ||
         memcmp(frame.data[0], source, sizeof(source)) != 0))
        return 2;
    free(frame.data[0]);
    return 0;
}

int main(void)
{
    size_t samples = 0;
    size_t bytes = 0;
    if (libopenjoc_pcm_payload_bytes(2, 2, &samples, &bytes) != 0 ||
        samples != 4 || bytes != 4 * sizeof(float))
        return 10;
    if (libopenjoc_pcm_payload_bytes(SIZE_MAX / 2 + 1, 2,
                                    &samples, &bytes) != AVERROR_INVALIDDATA)
        return 11;
    if (libopenjoc_pcm_payload_bytes(SIZE_MAX / sizeof(float) + 1, 1,
                                    &samples, &bytes) != AVERROR_INVALIDDATA)
        return 12;
    if (libopenjoc_pcm_payload_bytes(1, 0, &samples, &bytes) != AVERROR_INVALIDDATA)
        return 13;
    if (check_frame(4 * sizeof(float), 0) != 0)
        return 20;
    if (check_frame(4, AVERROR_INVALIDDATA) != 0)
        return 21;
    if (check_frame(4 * sizeof(float) - 1, AVERROR_INVALIDDATA) != 0)
        return 22;
    if (check_frame(4 * sizeof(float) + 1, AVERROR_INVALIDDATA) != 0)
        return 23;
    return 0;
}
"""

        with tempfile.TemporaryDirectory(prefix="openjoc-pcm-contract-") as temporary:
            source_path = Path(temporary) / "pcm-contract.c"
            binary_name = "pcm-contract.exe" if os.name == "nt" else "pcm-contract"
            binary_path = Path(temporary) / binary_name
            source_path.write_text(harness, encoding="utf-8")
            environment = os.environ.copy()
            if os.name == "nt":
                compiler_path = shutil.which(compiler[0]) or compiler[0]
                environment["PATH"] = (
                    str(Path(compiler_path).resolve().parent)
                    + os.pathsep
                    + environment.get("PATH", "")
                )
            build = subprocess.run(
                [*compiler, "-std=c11", "-Wall", "-Wextra", "-Werror",
                 "-Wno-sign-compare",
                 str(source_path), "-o", str(binary_path)],
                capture_output=True,
                text=True,
                check=False,
                env=environment,
            )
            self.assertEqual(
                build.returncode,
                0,
                f"compiler={compiler!r}\n{build.stdout}{build.stderr}",
            )
            run = subprocess.run(
                [str(binary_path)],
                capture_output=True,
                text=True,
                check=False,
                env=environment,
            )
            self.assertEqual(run.returncode, 0, run.stderr)


if __name__ == "__main__":
    unittest.main()
