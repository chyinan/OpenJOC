"""Compile and exercise the mpv AVFrame clone-only transport metadata hunk."""

from __future__ import annotations

import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PATCH_PATH = ROOT / "integrations/mpv/patches/mpv-0.41.0-openjoc.patch"


def extract_clone_block(patch: str) -> str:
    match = re.search(
        r"(?ms)^\+    AVFrame \*clone = av_frame_clone\(frame->av_frame\);"
        r".*?^\+    return clone;\n",
        patch,
    )
    if match is None:
        raise AssertionError("clone-only channel-layout normalization is missing")
    return "\n".join(line[1:] for line in match.group(0).splitlines()) + "\n"


def compiler_command() -> list[str] | None:
    configured = os.environ.get("CC", "").strip()
    if configured:
        return shlex.split(configured)
    for name in ("cc", "clang", "gcc"):
        path = shutil.which(name)
        if path:
            return [path]
    return None


HARNESS_PREFIX = r"""
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MP_MAX_CHANNELS 24
enum {
    FL = 1, FR, FC, LFE, BL, BR, SL, SR,
    TFL, TFR, TBL, TBR,
    BIL = 61, BIR = 62,
};

struct mp_chmap {
    int num;
    int speaker[MP_MAX_CHANNELS];
};
typedef struct AVChannelLayout {
    int nb_channels;
    int valid;
    int speaker[MP_MAX_CHANNELS];
} AVChannelLayout;
typedef struct AVFrame {
    AVChannelLayout ch_layout;
    int sample_rate;
    int format;
    int nb_samples;
    int64_t pts;
    uint32_t pcm[64];
} AVFrame;
struct mp_aframe {
    AVFrame *av_frame;
    struct mp_chmap chmap;
    double speed;
};

static AVFrame *av_frame_clone(const AVFrame *source)
{
    AVFrame *clone = malloc(sizeof(*clone));
    if (clone)
        *clone = *source;
    return clone;
}

static void av_channel_layout_uninit(AVChannelLayout *layout)
{
    memset(layout, 0, sizeof(*layout));
}

static int av_channel_layout_check(const AVChannelLayout *layout)
{
    if (!layout->valid || layout->nb_channels <= 0 ||
        layout->nb_channels > MP_MAX_CHANNELS)
        return 0;
    int previous = -1;
    for (int i = 0; i < layout->nb_channels; i++) {
        int speaker = layout->speaker[i];
        if (speaker < 0 || speaker >= 64 || speaker <= previous)
            return 0;
        previous = speaker;
    }
    return 1;
}

static int mp_chmap_from_av_layout(struct mp_chmap *map,
                                   const AVChannelLayout *layout)
{
    if (!av_channel_layout_check(layout))
        return 0;
    map->num = layout->nb_channels;
    memcpy(map->speaker, layout->speaker, (size_t)map->num * sizeof(int));
    return 1;
}

static int mp_chmap_equals(const struct mp_chmap *a,
                           const struct mp_chmap *b)
{
    return a->num == b->num &&
           memcmp(a->speaker, b->speaker, (size_t)a->num * sizeof(int)) == 0;
}

static void mp_chmap_to_av_layout(AVChannelLayout *layout,
                                  const struct mp_chmap *map)
{
    av_channel_layout_uninit(layout);
    if (map->num <= 0 || map->num > MP_MAX_CHANNELS || map->speaker[0] == 999)
        return;
    layout->nb_channels = map->num;
    layout->valid = 1;
    memcpy(layout->speaker, map->speaker, (size_t)map->num * sizeof(int));
}

static int mp_chmap_is_lavc(const struct mp_chmap *map)
{
    if (map->num <= 0 || map->num > MP_MAX_CHANNELS)
        return 0;
    int previous = -1;
    for (int i = 0; i < map->num; i++) {
        int speaker = map->speaker[i];
        if (speaker < 0 || speaker >= 64 || speaker <= previous)
            return 0;
        previous = speaker;
    }
    return 1;
}

"""


HARNESS_SUFFIX = r"""
static int run_case(const char *name, const int *av_map, int av_count,
                    int av_valid, const int *mp_map, int mp_count,
                    int expected_changed, const int *expected_map,
                    int expected_count, int expected_valid)
{
    AVFrame input = {
        .ch_layout = { .nb_channels = av_count, .valid = av_valid },
        .sample_rate = 48000,
        .format = 1,
        .nb_samples = 3,
        .pts = 123456,
        .pcm = { 0x3e800001, 0xbf000002, 0x7fc01234, 0x80000000,
                 0x3f000000, 0x3e000003, 0x00000001 },
    };
    memcpy(input.ch_layout.speaker, av_map, (size_t)av_count * sizeof(int));
    struct mp_aframe frame = { .av_frame = &input, .speed = 1.0 };
    frame.chmap.num = mp_count;
    memcpy(frame.chmap.speaker, mp_map, (size_t)mp_count * sizeof(int));
    AVFrame *clone = mp_aframe_to_avframe(&frame);
    if (!clone) {
        fprintf(stderr, "%s: clone was NULL\n", name);
        return 1;
    }
    if (clone->ch_layout.nb_channels != expected_count ||
        clone->ch_layout.valid != expected_valid ||
        memcmp(clone->ch_layout.speaker, expected_map,
               (size_t)expected_count * sizeof(int)) != 0) {
        fprintf(stderr, "%s: clone transport map differs\n", name);
        free(clone);
        return 2;
    }
    if (input.ch_layout.nb_channels != av_count ||
        input.ch_layout.valid != av_valid ||
        memcmp(input.ch_layout.speaker, av_map,
               (size_t)av_count * sizeof(int)) != 0) {
        fprintf(stderr, "%s: source AVFrame layout was mutated\n", name);
        free(clone);
        return 3;
    }
    if (memcmp(clone->pcm, input.pcm, sizeof(input.pcm)) != 0 ||
        clone->sample_rate != 48000 || clone->format != 1 ||
        clone->nb_samples != 3 || clone->pts != 123456) {
        fprintf(stderr, "%s: clone PCM or frame attributes changed\n", name);
        free(clone);
        return 4;
    }
    int changed = memcmp(&clone->ch_layout, &input.ch_layout,
                         sizeof(input.ch_layout)) != 0;
    if (changed != expected_changed) {
        fprintf(stderr, "%s: unexpected map remap decision\n", name);
        free(clone);
        return 5;
    }
    free(clone);
    return 0;
}

int main(void)
{
    static const int binaural[] = { BIL, BIR };
    static const int stereo[] = { FL, FR };
    static const int layout_7_1_4[] = {
        FL, FR, FC, LFE, BL, BR, SL, SR, TFL, TFR, TBL, TBR
    };
    static const int layout_22_2[] = {
        9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
    };
    static const int valid_custom[] = { FL, FR, FC, LFE, BL, BR, 33, 34 };
    int result = 0;
    result |= run_case("BIL/BIR-to-stereo", binaural, 2, 1,
                       stereo, 2, 1, stereo, 2, 1);
    result |= run_case("stereo-stable", stereo, 2, 1,
                       stereo, 2, 0, stereo, 2, 1);
    result |= run_case("7.1.4-stable", layout_7_1_4, 12, 1,
                       layout_7_1_4, 12, 0, layout_7_1_4, 12, 1);
    result |= run_case("22.2-stable", layout_22_2, 24, 1,
                       layout_22_2, 24, 0, layout_22_2, 24, 1);
    result |= run_case("valid-custom-stable", valid_custom, 8, 1,
                       valid_custom, 8, 0, valid_custom, 8, 1);
    result |= run_case("unmappable-source-kept", binaural, 2, 0,
                       stereo, 2, 0, binaural, 2, 0);
    return result;
}
"""


class MpvAframeTransportPatchTests(unittest.TestCase):
    def test_clone_hunk_only_normalizes_divergent_layout_metadata(self) -> None:
        patch = PATCH_PATH.read_text(encoding="utf-8")
        self.assertIn("mp_chmap_from_av_layout(&cloned_map, &clone->ch_layout)", patch)
        self.assertIn("mp_chmap_to_av_layout(&expected_layout, &frame->chmap)", patch)
        self.assertIn("av_channel_layout_uninit(&clone->ch_layout)", patch)
        self.assertIn("AVFrame *clone = av_frame_clone(frame->av_frame);", patch)
        self.assertNotIn("memcpy(clone->", patch)
        self.assertNotIn("av_frame_make_writable", patch)

        compiler = compiler_command()
        if compiler is None:
            self.skipTest("a C compiler is required for the AVFrame clone transport test")
        clone_block = extract_clone_block(patch)
        function = (
            "static AVFrame *mp_aframe_to_avframe(struct mp_aframe *frame)\n"
            "{\n"
            "    if (!frame || !mp_chmap_is_lavc(&frame->chmap)) return NULL;\n"
            + clone_block
            + "}\n"
        )
        source = HARNESS_PREFIX + function + HARNESS_SUFFIX
        with tempfile.TemporaryDirectory(prefix="mpv-aframe-transport-") as temporary_name:
            temporary = Path(temporary_name)
            c_file = temporary / "transport.c"
            executable = temporary / "transport"
            c_file.write_text(source, encoding="utf-8")
            build = subprocess.run(
                [*compiler, "-std=c11", "-Wall", "-Wextra", "-Werror", str(c_file), "-o", str(executable)],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                errors="replace",
            )
            self.assertEqual(build.returncode, 0, build.stdout)
            result = subprocess.run(
                [str(executable)],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                errors="replace",
            )
            self.assertEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()
