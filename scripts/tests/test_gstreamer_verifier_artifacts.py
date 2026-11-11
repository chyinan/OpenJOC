"""Exercise the POSIX verifier boundary with explicit Cargo/GStreamer doubles."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ("verify-gstreamer.sh", "verify-gstreamer-autoplug.sh",
           "verify-gstreamer-output-targets.sh")

TOOL = r'''#!PYTHON
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
log = pathlib.Path(os.environ['TEST_LOG'])
record = {'tool': name, 'argv': sys.argv[1:]}
if name == 'cargo':
    record['target_dir'] = os.environ['CARGO_TARGET_DIR']
    with log.open('a') as out: out.write(json.dumps(record) + '\n')
    if os.environ.get('FAIL_STAGE') == 'cargo': sys.exit(17)
    directory = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    target = os.environ.get('CARGO_BUILD_TARGET') or os.environ.get('CONFIGURED_TARGET')
    if target: directory = directory / target
    directory = directory / 'release'
    directory.mkdir(parents=True, exist_ok=True)
    plugin = directory / os.environ.get('PLUGIN_NAME', 'libgstopenjoc.so')
    if os.environ.get('ARTIFACT_MODE') != 'absent_file': plugin.write_text('CURRENT')
    item = {'reason': 'compiler-artifact', 'manifest_path': str(pathlib.Path(sys.argv[sys.argv.index('--manifest-path') + 1]).parent / 'crates/gst-plugin-openjoc/Cargo.toml'), 'target': {'name': 'gstopenjoc', 'crate_types': ['cdylib', 'rlib']},
            'filenames': [str(plugin.resolve()), str(directory / 'libgstopenjoc.rlib')], 'fresh': os.environ.get('FRESH') == '1'}
    mode = os.environ.get('ARTIFACT_MODE')
    if mode == 'none': sys.exit(0)
    if mode == 'malformed': print('{broken'); sys.exit(0)
    if mode == 'wrong_target': item['target']['name'] = 'unrelated'
    if mode == 'wrong_type': item['target']['crate_types'] = ['rlib']
    if mode == 'wrong_manifest': item['manifest_path'] = '/unused/Cargo.toml'
    print(json.dumps({'reason': 'build-script-executed'}))
    print(json.dumps({'reason': 'compiler-artifact', 'target': {'name':'other', 'crate_types':['cdylib']}, 'filenames':['/unused/other.so']}))
    print(json.dumps(item))
    if mode == 'duplicate': print(json.dumps(item))
    if mode == 'multiple':
        other = directory / 'other' / plugin.name
        other.parent.mkdir(); other.write_text('OTHER')
        item['filenames'] = [str(other.resolve())]
        print(json.dumps(item))
    sys.exit(0)
record['plugin_path'] = os.environ.get('GST_PLUGIN_PATH', '')
selected = None
for directory in record['plugin_path'].split(os.pathsep):
    for filename in ('libgstopenjoc.so','libgstopenjoc.dylib','gstopenjoc.dll'):
        plugin = pathlib.Path(directory) / filename
        if plugin.is_file(): selected = plugin.read_text(); break
    if selected: break
record['selected'] = selected
with log.open('a') as out: out.write(json.dumps(record) + '\n')
if not selected: sys.exit(23)
if os.environ.get('FAIL_STAGE') == name: sys.exit(19)
if name == 'gst-launch-1.0':
    print('openjocclassify')
    if not any('ordinary.ec3' in arg for arg in sys.argv): print('started OpenJOC session')
    print('Got EOS')
    for target, channels in [('2.0',2),('5.1',6),('7.1.4',12)]:
        if any('channels='+str(channels)+',' in arg for arg in sys.argv):
            print('target=speaker:'+target+' channels=(int)'+str(channels))
'''


@unittest.skipUnless(os.name == "posix", "requires POSIX shell verifier scripts")
class GStreamerArtifactTests(unittest.TestCase):
    def run_verifier(self, script, *, target=None, relative=False, stale=False,
                     artifact_mode=None, failure=None, configured=False,
                     plugin_name="libgstopenjoc.so", fresh=False, ordinary=False):
        with tempfile.TemporaryDirectory(prefix="openjoc gst 日本語 ") as temporary:
            base = Path(temporary)
            repo = base / "source tree"
            (repo / "scripts").mkdir(parents=True)
            for filename in SCRIPTS + ("build-gstreamer-plugin.py",):
                source = ROOT / "scripts" / filename
                if source.exists(): shutil.copy2(source, repo / "scripts" / filename)
            (repo / "Cargo.toml").write_text("# synthetic command-boundary fixture\n")
            fixture = base / "joc with spaces.ec3"
            fixture.write_text("Synthetic bytes; no native decoder is used.")
            ordinary_fixture = base / "ordinary.ec3"
            ordinary_fixture.write_text("Synthetic ordinary bytes.")
            tools = base / "tools"; tools.mkdir()
            for tool in ("cargo", "gst-inspect-1.0", "gst-launch-1.0"):
                path = tools / tool
                path.write_text(TOOL.replace("#!PYTHON", "#!" + sys.executable))
                path.chmod(0o755)
            target_root = base / "build 日本語 with spaces"
            legacy = target_root / "release" / plugin_name
            if stale:
                legacy.parent.mkdir(parents=True)
                legacy.write_text("STALE")
            log = base / "commands.jsonl"
            env = os.environ.copy()
            for key in ("CARGO_BUILD_TARGET", "OPENJOC_GSTREAMER_TARGET_DIR", "GST_PLUGIN_PATH",
                        "ARTIFACT_MODE", "FAIL_STAGE", "CONFIGURED_TARGET"):
                env.pop(key, None)
            env.update(PATH=str(tools) + os.pathsep + env["PATH"], TEST_LOG=str(log),
                       OPENJOC_GSTREAMER_TARGET_DIR=str(target_root.relative_to(base) if relative else target_root),
                       GST_PLUGIN_PATH=str(base / "inherited plugins"), PLUGIN_NAME=plugin_name,
                       TMPDIR=str(base), FRESH="1" if fresh else "0")
            if target: env["CONFIGURED_TARGET" if configured else "CARGO_BUILD_TARGET"] = target
            if artifact_mode: env["ARTIFACT_MODE"] = artifact_mode
            if failure: env["FAIL_STAGE"] = failure
            command = ["sh", str(repo / "scripts" / script), str(fixture)]
            if ordinary: command.append(str(ordinary_fixture))
            result = subprocess.run(command, cwd=base, env=env, text=True, capture_output=True)
            records = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            if stale: self.assertEqual(legacy.read_text(), "STALE")
            return result, records

    def assert_current(self, result, records):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = [record for record in records if record['tool'] != 'cargo']
        self.assertTrue(calls)
        self.assertTrue(all(record['selected'] == 'CURRENT' for record in calls), calls)
        self.assertTrue(all(record['plugin_path'].endswith('inherited plugins') for record in calls))
        cargo = records[0]
        self.assertEqual(cargo['argv'][0], 'build')
        self.assertIn('--release', cargo['argv'])
        self.assertIn('gst-plugin-openjoc', cargo['argv'])
        self.assertIn('gstreamer', cargo['argv'])

    def test_default_layout_and_cached_artifact(self):
        for script in SCRIPTS:
            for fresh in (False, True):
                with self.subTest(script=script, fresh=fresh):
                    self.assert_current(*self.run_verifier(script, fresh=fresh))

    def test_explicit_host_target_uses_current_artifact(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                self.assert_current(*self.run_verifier(script, target='x86_64-unknown-linux-gnu'))

    def test_configured_target_does_not_validate_stale_plugin(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                self.assert_current(*self.run_verifier(script, target='x86_64-unknown-linux-gnu', configured=True, stale=True))

    def test_relative_unicode_spaced_target_path(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                self.assert_current(*self.run_verifier(script, target='x86_64-unknown-linux-gnu', relative=True))

    def test_cdylib_names_and_duplicate_records(self):
        for plugin in ('libgstopenjoc.so', 'libgstopenjoc.dylib', 'gstopenjoc.dll'):
            with self.subTest(plugin=plugin):
                self.assert_current(*self.run_verifier(SCRIPTS[0], plugin_name=plugin, artifact_mode='duplicate'))

    def test_missing_ambiguous_or_invalid_artifacts_fail_before_inspection(self):
        for script in SCRIPTS:
            for mode in ('none', 'multiple', 'malformed', 'wrong_target', 'wrong_type', 'wrong_manifest', 'absent_file'):
                with self.subTest(script=script, mode=mode):
                    result, records = self.run_verifier(script, artifact_mode=mode)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual([record['tool'] for record in records], ['cargo'])

    def test_each_command_failure_stops_the_verifier(self):
        for script in SCRIPTS:
            for stage, code in (('cargo',17), ('gst-inspect-1.0',19), ('gst-launch-1.0',19)):
                with self.subTest(script=script, stage=stage):
                    result, records = self.run_verifier(script, failure=stage)
                    self.assertNotEqual(result.returncode, 0)
                    if script != SCRIPTS[2] or stage != 'gst-launch-1.0': self.assertEqual(result.returncode, code)
                    self.assertEqual(sum(record['tool'] == stage for record in records), 1)

    def test_autoplug_keeps_ordinary_control_and_exact_fixture_arguments(self):
        result, records = self.run_verifier(SCRIPTS[1], target='host-triple', ordinary=True)
        self.assert_current(result, records)
        launches = [record for record in records if record['tool'] == 'gst-launch-1.0']
        self.assertEqual(len(launches), 2)
        self.assertTrue(any(arg.endswith('joc with spaces.ec3') for arg in launches[0]['argv']))
        self.assertTrue(any(arg.endswith('ordinary.ec3') for arg in launches[1]['argv']))

    def test_output_targets_keep_all_exact_caps(self):
        result, records = self.run_verifier(SCRIPTS[2], target='host-triple')
        self.assert_current(result, records)
        launches = [record for record in records if record['tool'] == 'gst-launch-1.0']
        self.assertEqual(len(launches), 3)
        for record, channels, mask in zip(launches, (2,6,12), ('0x3','0xc0f','0x33c3f')):
            self.assertIn('audio/x-raw,format=F32LE,rate=48000,layout=interleaved,channels='+str(channels)+',channel-mask=(bitmask)'+mask, record['argv'])


if __name__ == '__main__':
    unittest.main()
