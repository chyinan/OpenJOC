# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

"""Compile the exact scoped lavfi persistence hunks from both pinned patches."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PATCHES = [ROOT / "integrations/mpv/patches" / name for name in (
    "mpv-0.41.0-openjoc.patch", "mpv-master-openjoc.patch",
)]


def added_block(patch: str, pattern: str) -> str:
    match = re.search(pattern, patch, re.MULTILINE | re.DOTALL)
    if not match:
        raise AssertionError("native gain persistence block is missing")
    return "\n".join(line[1:] for line in match.group().splitlines()) + "\n"


def production_blocks(patch: str) -> tuple[str, str, str]:
    graph = added_block(patch, r"^\+static bool is_openjoc_gain_graph\(.*?^\+}\n")
    command = added_block(patch, r"^\+static bool is_openjoc_gain_command\(.*?^\+}\n")
    update = added_block(patch, r"^\+        bool ok = avfilter_graph_send_command.*?^\+        return ok;\n")
    replay = added_block(patch, r"^\+        if \(c->openjoc_gain_volume && is_openjoc_gain_graph\(c\).*?^\+        }\n")
    return graph + command, update, replay


PREFIX = r'''
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#define MP_FRAME_AUDIO 1
#define MP_FRAME_VIDEO 2
#define MP_HANDLE_OOM(p) assert(p)
#define MP_FATAL(c, ...) ((c)->errors++)
struct graph { const char *argument; float factor; };
struct lavfi {
    char *graph_string;
    char *openjoc_gain_volume;
    bool direct_filter, force_bidir;
    int force_type;
    struct graph *graph;
    bool initialized, failed;
    int errors;
};
struct mp_filter_command { const char *target, *cmd, *arg; };
static bool fail_command;
static int commands, allocations, frees;
static const char *last_target;
static const char *last_argument;
static char *talloc_strdup(void *owner, const char *arg)
{
    (void)owner;
    char *result = malloc(strlen(arg) + 1);
    assert(result);
    strcpy(result, arg);
    allocations++;
    return result;
}
static void talloc_free(void *data)
{
    if (data) { free(data); frees++; }
}
static int avfilter_graph_send_command(struct graph *graph, const char *target,
                                      const char *command, const char *arg,
                                      char *result, int size, int flags)
{
    (void)command; (void)result; (void)size; (void)flags;
    commands++;
    last_target = target;
    last_argument = arg;
    if (fail_command) return -1;
    graph->argument = arg;
    graph->factor = (float)strtod(arg, NULL);
    return 0;
}
static void free_graph(struct lavfi *c)
{
    c->graph = NULL;
    c->initialized = false;
}
'''

SUFFIX = r'''
static struct lavfi owned(struct graph *graph)
{
    return (struct lavfi){
        .graph_string = "volume@openjoc_gain=volume=1.0115794542598986:precision=float",
        .force_type = MP_FRAME_AUDIO, .force_bidir = true,
        .graph = graph, .initialized = true,
    };
}
static void destroy_instance(struct lavfi *c)
{
    free_graph(c);
    talloc_free(c->openjoc_gain_volume);
    c->openjoc_gain_volume = NULL;
}
static void rebuild(struct lavfi *c, struct graph *new_graph)
{
    // Same-instance format/EOF/reset recovery preserves the per-instance cache.
    free_graph(c);
    *new_graph = (struct graph){ .factor = (float)1.0115794542598986 };
    c->graph = new_graph;
    init_new_graph(c);
}
int main(void)
{
    struct graph graph = { .factor = (float)1.0115794542598986 };
    struct lavfi c = owned(&graph);
    const char *plus02 = "1.0232929922807541";
    const char *plus03 = "1.0351421666793439";
    struct mp_filter_command cmd = { "volume", "volume", plus02 };
    assert(is_openjoc_gain_graph(&c) && is_openjoc_gain_command(&cmd));
    assert(run_command(&c, &cmd));
    assert(c.openjoc_gain_volume && !strcmp(c.openjoc_gain_volume, plus02));
    assert(!strcmp(c.graph_string, "volume@openjoc_gain=volume=1.0115794542598986:precision=float"));
    char *successful = c.openjoc_gain_volume;
    fail_command = true;
    cmd.arg = plus03;
    assert(!run_command(&c, &cmd) && c.openjoc_gain_volume == successful);
    cmd.arg = "invalid";
    assert(!run_command(&c, &cmd) && c.openjoc_gain_volume == successful);
    fail_command = false;
    for (int context = 0; context < 6; context++) {
        rebuild(&c, &graph);
        assert(c.initialized && !c.failed);
        assert(!strcmp(last_target, "volume@openjoc_gain"));
        assert(!strcmp(last_argument, plus02));
        // First output uses exactly the existing FFmpeg float-factor arithmetic.
        float first_sample = (float)0.25 * graph.factor;
        assert(first_sample == (float)0.25 * (float)strtod(plus02, NULL));
        assert(c.openjoc_gain_volume == successful);
    }
    // Newer previews and Cancel replace the scalar cache, with no queued replay.
    cmd = (struct mp_filter_command){ "volume", "volume", plus03 };
    assert(run_command(&c, &cmd));
    cmd.arg = "1.0115794542598986";
    assert(run_command(&c, &cmd));
    rebuild(&c, &graph);
    assert(!strcmp(last_argument, cmd.arg));
    assert(graph.factor == (float)strtod(cmd.arg, NULL));

    const char *noneligible[] = { "0.099", "10.001", "nan", "inf", "1/2", "1 " };
    for (size_t i = 0; i < sizeof(noneligible)/sizeof(noneligible[0]); i++) {
        cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
        assert(run_command(&c, &cmd) && c.openjoc_gain_volume);
        cmd.arg = noneligible[i];
        assert(!is_openjoc_gain_command(&cmd));
        assert(run_command(&c, &cmd) && !c.openjoc_gain_volume);
        int before = commands;
        rebuild(&c, &graph);
        assert(commands == before); // cannot resurrect an older numeric target
    }
    const char *other_targets[] = { "all", "volume@openjoc_gain" };
    for (size_t i = 0; i < sizeof(other_targets)/sizeof(other_targets[0]); i++) {
        cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
        assert(run_command(&c, &cmd));
        cmd.target = other_targets[i];
        assert(run_command(&c, &cmd) && !c.openjoc_gain_volume);
    }
    cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
    assert(run_command(&c, &cmd));
    cmd.cmd = "other-command";
    assert(run_command(&c, &cmd) && !c.openjoc_gain_volume);
    for (int null_field = 0; null_field < 3; null_field++) {
        cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
        if (null_field == 0) cmd.target = NULL;
        if (null_field == 1) cmd.cmd = NULL;
        if (null_field == 2) cmd.arg = NULL;
        assert(!is_openjoc_gain_command(&cmd));
    }
    const char *other_graphs[] = {
        "volume=1:precision=float", "volume@other=volume=1:precision=float",
        "anull,volume@openjoc_gain=volume=1:precision=float",
        "volume@openjoc_gain=volume=1:precision=float,anull",
        "volume@openjoc_gain=volume=1:precision=double",
        "volume@openjoc_gain=volume=0.09:precision=float",
        "volume@openjoc_gain=volume=11:precision=float",
        "volume@openjoc_gain=volume=nan:precision=float",
        "volume@openjoc_gain=volume=1oops:precision=float", NULL,
    };
    for (size_t i = 0; i < sizeof(other_graphs)/sizeof(other_graphs[0]); i++) {
        struct lavfi other = owned(&graph);
        other.graph_string = (char *)other_graphs[i];
        assert(!is_openjoc_gain_graph(&other));
        cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
        assert(run_command(&other, &cmd) && !other.openjoc_gain_volume);
        int before = commands;
        rebuild(&other, &graph);
        assert(commands == before);
    }
    struct lavfi other = owned(&graph);
    other.direct_filter = true;
    assert(!is_openjoc_gain_graph(&other));
    other.direct_filter = false; other.force_bidir = false;
    assert(!is_openjoc_gain_graph(&other));
    other.force_bidir = true; other.force_type = MP_FRAME_VIDEO;
    assert(!is_openjoc_gain_graph(&other));

    // Replay error is explicit and prevents this graph from processing samples;
    // mpv's normal failed-user-filter policy may subsequently bypass the stage.
    cmd = (struct mp_filter_command){ "volume", "volume", plus02 };
    assert(run_command(&c, &cmd));
    fail_command = true;
    rebuild(&c, &graph);
    assert(c.failed && !c.initialized && c.graph == NULL && c.errors == 1);
    assert(c.openjoc_gain_volume && !strcmp(c.openjoc_gain_volume, plus02));
    fail_command = false;
    // Apply/file/track replacement is a new instance and must use its own
    // constructor target, never the destroyed instance's last runtime target.
    destroy_instance(&c);
    c = owned(&graph);
    int before = commands;
    rebuild(&c, &graph);
    assert(commands == before && graph.factor == (float)1.0115794542598986);
    destroy_instance(&c);
    assert(allocations == frees);
    puts("OpenJOC lavfi gain persistence C checks passed");
    return 0;
}
'''


class MpvGainPersistencePatchTests(unittest.TestCase):
    def test_patch_pins_and_narrow_replay_contract(self) -> None:
        baseline = dict(line.split("=", 1) for line in
                        (ROOT / "integrations/ffmpeg/native/BASELINES").read_text().splitlines()
                        if "=" in line)
        manifest = json.loads((ROOT / "packaging/player/PLAYER_PACKAGE_MANIFEST.json").read_text())
        self.assertEqual(manifest["pinned_stack"]["mpv"]["patch_sha256"],
                         hashlib.sha256(PATCHES[0].read_bytes()).hexdigest())
        for path, key in zip(PATCHES, ("MPV_STABLE_PATCH_SHA256", "MPV_MASTER_PATCH_SHA256")):
            patch = path.read_text()
            self.assertEqual(baseline[key], hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertIn('"volume@openjoc_gain", "volume"', patch)
            self.assertIn("MP_HANDLE_OOM(c->openjoc_gain_volume)", patch)
            # Replay is inserted before timebase setup and initialized=true;
            # the hunk's context fixes its placement after graph_config failure.
            replay = patch.index('+        if (c->openjoc_gain_volume &&')
            self.assertLess(replay, patch.index(' // The timebase is available after configuring.', replay))
            self.assertNotIn("gain_context_timer", (ROOT / "integrations/mpv/openjoc-settings.lua").read_text())

    def test_exact_production_hunks_with_c_compiler(self) -> None:
        configured = os.environ.get("CC", "").strip()
        compiler = shlex.split(configured) if configured else [shutil.which("cc") or ""]
        if not compiler[0]:
            self.skipTest("a C compiler is unavailable")
        blocks = [production_blocks(path.read_text()) for path in PATCHES]
        self.assertEqual(blocks[0], blocks[1], "stable/master persistence semantics drifted")
        helpers, update, replay = blocks[0]
        source = PREFIX + helpers
        source += "static bool run_command(struct lavfi *c, struct mp_filter_command *cmd)\n{\n" + update + "}\n"
        source += "static void init_new_graph(struct lavfi *c)\n{\n" + replay + "    c->initialized = true;\n}\n"
        source += SUFFIX
        with tempfile.TemporaryDirectory(prefix="openjoc-mpv-gain-persist-") as directory:
            cfile, executable = Path(directory) / "test.c", Path(directory) / "test"
            cfile.write_text(source)
            subprocess.run(compiler + ["-std=c11", "-Wall", "-Wextra", "-Werror",
                                       str(cfile), "-lm", "-o", str(executable)], check=True)
            result = subprocess.run([str(executable)], capture_output=True, text=True, check=True)
            self.assertIn("OpenJOC lavfi gain persistence C checks passed", result.stdout)


class MpvGainRecoveryEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        path = ROOT / "scripts/verify-mpv-gain-recovery.py"
        spec = importlib.util.spec_from_file_location("mpv_gain_recovery", path)
        assert spec and spec.loader
        cls.verifier = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.verifier)

    @staticmethod
    def stage(name: str) -> str:
        return "GAIN_RECOVERY_STAGE " + name + "\n"

    @staticmethod
    def volume(value: str, point: str = "nan") -> str:
        return "volume@openjoc_gain: n:" + point + " t:nan pts:nan volume_dB:" + value + "\n"

    def pair(self, phase: str, target: str = "0.200000") -> str:
        return self.stage(phase) + self.volume("0.100000") + self.volume(target)

    def ordinary_log(self) -> str:
        return (self.stage("PREVIEW_02") + self.pair("PRE_FORMAT_44100")
                + self.stage("POST_FORMAT_44100") + self.pair("PRE_FORMAT_32000")
                + self.stage("DONE"))

    def test_checker_rejects_early_stale_pair_even_when_last_pair_is_correct(self) -> None:
        log = self.ordinary_log()
        self.verifier.verify_log(log, "saved")
        for mutation in (
            log.replace(self.volume("0.200000"), "", 1),
            log.replace(self.volume("0.200000"), self.volume("0.200000", "1.000000"), 1),
            log.replace(self.volume("0.200000"), self.volume("0.200001"), 1),
        ):
            with self.assertRaisesRegex(ValueError, "latest target"):
                self.verifier.verify_log(mutation, "saved")

    def test_checker_requires_actual_scenario_context_and_intent(self) -> None:
        good = self.ordinary_log()
        with self.assertRaisesRegex(ValueError, "required context"):
            self.verifier.verify_log(good + "GAIN_RECOVERY_SEEK seekable=true\n", "seek")
        with self.assertRaisesRegex(ValueError, "paused/resume"):
            self.verifier.verify_log(good, "paused")
        with self.assertRaisesRegex(ValueError, "intent transition"):
            self.verifier.verify_log(good, "cancel")
        with self.assertRaisesRegex(ValueError, "intent transition"):
            self.verifier.verify_log(good, "new-preview")
        seek = (self.stage("PREVIEW_02") + self.pair("PRE_FORMAT_44100")
                + self.pair("PRE_SEEK") + "GAIN_RECOVERY_SEEK seekable=true\n" + self.stage("DONE"))
        self.verifier.verify_log(seek, "seek")
        paused = (self.stage("PREVIEW_02") + self.pair("RESUME")
                  + self.pair("PRE_FORMAT_32000") + self.pair("PRE_FORMAT_44100") + self.stage("DONE"))
        self.verifier.verify_log(paused, "paused")

    def test_checker_tracks_cancel_and_newer_preview_exactly(self) -> None:
        for scenario, stage, expected in (("cancel", "CANCEL_01", "0.100000"),
                                           ("new-preview", "PREVIEW_03", "0.300000")):
            log = (self.stage("PREVIEW_02") + self.pair("PRE_FORMAT_44100")
                   + self.stage(stage) + self.pair("PRE_FORMAT_32000", expected) + self.stage("DONE"))
            self.verifier.verify_log(log, scenario)
            with self.assertRaisesRegex(ValueError, "latest target"):
                self.verifier.verify_log(log.replace(self.volume(expected), self.volume("0.200000"), 1)
                                         if expected != "0.100000" else
                                         log.replace(self.stage(stage) + self.pair("PRE_FORMAT_32000", expected),
                                                     self.stage(stage) + self.pair("PRE_FORMAT_32000", "0.200000")),
                                         scenario)

    def test_real_harness_is_in_the_packaged_verification_path(self) -> None:
        source = (ROOT / "integrations/mpv/verify-player.sh").read_text()
        self.assertIn('scripts/verify-mpv-gain-recovery.py" "$mpv" "$live_gain_fixture"', source)
        driver = (ROOT / "integrations/mpv/test-openjoc-gain-recovery-driver.lua").read_text()
        self.assertIn("mp.get_property_native('seekable') == true", driver)
        self.assertIn("playback_restarts > restarts", driver)
        self.assertIn("@recovery_fixed_ao:lavfi=[aresample=48000]", driver)
        self.assertIn("NON_OPENJOC_TRACK_NO_GAIN", driver)
        self.assertNotIn("'af', 'remove', '@openjoc_gain'", driver)


if __name__ == "__main__":
    unittest.main()
