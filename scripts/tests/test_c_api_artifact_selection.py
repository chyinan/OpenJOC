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
TOOLS = ('sh', 'cc', 'c++', 'ar', 'python3')


@unittest.skipUnless(os.name != 'nt' and all(shutil.which(tool) for tool in TOOLS),
                     'requires POSIX shell, C/C++ compilers, ar and Python 3')
class CApiArtifactSelectionTests(unittest.TestCase):
    """Real shell and C linking; Cargo, fixtures and ABI consumers are test doubles."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='OpenJOC C API ')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.repo = self.root / 'repo with spaces'
        (self.repo / 'scripts').mkdir(parents=True)
        shutil.copy2(ROOT / 'scripts/test-c-api.sh', self.repo / 'scripts/test-c-api.sh')
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        self.log = self.root / 'commands.jsonl'
        self.environment = {**os.environ, 'PATH': f'{self.tools}{os.pathsep}{os.environ["PATH"]}',
                            'TEST_COMMAND_LOG': str(self.log)}
        for variable in ('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_TARGET_DIR'):
            self.environment.pop(variable, None)
        self.write_tool('cargo', '''import json, os, sys
with open(os.environ['TEST_COMMAND_LOG'], 'a', encoding='utf-8') as stream:
    stream.write(json.dumps(['cargo', *sys.argv[1:]]) + '\\n')
print(os.environ.get('TEST_CARGO_MESSAGES', ''))
sys.exit(int(os.environ.get('TEST_CARGO_EXIT', '0')))
''')
        for compiler in ('cc', 'c++'):
            executable = shutil.which(compiler)
            self.write_tool(compiler, f'''import json, os, sys
with open(os.environ['TEST_COMMAND_LOG'], 'a', encoding='utf-8') as stream:
    stream.write(json.dumps([{compiler!r}, *sys.argv[1:]]) + '\\n')
if os.environ.get('TEST_FAIL_COMPILER') == {compiler!r}:
    sys.exit(23)
os.execv({executable!r}, [{executable!r}, *sys.argv[1:]])
''')
        capi = self.repo / 'crates/openjoc-capi'
        for directory in ('include', 'examples', 'tests/fixtures'):
            (capi / directory).mkdir(parents=True)
        source = '#include <stdio.h>\n#include <stdlib.h>\nextern int selected_library(void);\nint main(void) { printf("selected=%d\\n", selected_library()); return getenv("TEST_FAIL_CONSUMER") ? 29 : 0; }\n'
        for filename in ('examples/c_api_example.c', 'tests/orientation_22_2_caller.c', 'tests/abi13_caller.c'):
            (capi / filename).write_text(source, encoding='utf-8')
        (capi / 'examples/c_api_header.cpp').write_text('static_assert(sizeof(int) >= 2, "int width");\n', encoding='utf-8')
        generator = self.repo / 'scripts/generate-player-fixtures.sh'
        generator.write_text('#!/bin/sh\nset -eu\n[ -z "${TEST_FAIL_FIXTURES:-}" ] || exit 31\nprintf fixture > "$1/joc.ec3"\n', encoding='utf-8')
        generator.chmod(0o755)

    def write_tool(self, name, body):
        tool = self.tools / name
        tool.write_text(f'#!{sys.executable}\n' + body, encoding='utf-8')
        tool.chmod(0o755)

    def static_library(self, relative, marker):
        library = self.repo / relative / 'libopenjoc_capi.a'
        library.parent.mkdir(parents=True, exist_ok=True)
        source = self.root / f'library-{marker}.c'
        source.write_text(f'int selected_library(void) {{ return {marker}; }}\n', encoding='utf-8')
        obj = source.with_suffix('.o')
        subprocess.run([shutil.which('cc'), '-c', str(source), '-o', str(obj)], check=True, capture_output=True)
        subprocess.run([shutil.which('ar'), 'rcs', str(library), str(obj)], check=True, capture_output=True)
        return library

    def artifact(self, library, name='openjoc_capi', fresh=False):
        return {'reason': 'compiler-artifact', 'target': {'name': name, 'crate_types': ['rlib', 'staticlib', 'cdylib']},
                'filenames': [str(library.with_suffix('.rlib')), str(library), str(library.with_suffix('.so'))], 'fresh': fresh}

    def run_script(self, messages, exit_code=0):
        environment = {**self.environment, 'TEST_CARGO_MESSAGES': '\n'.join(json.dumps(value) for value in messages),
                       'TEST_CARGO_EXIT': str(exit_code)}
        return subprocess.run(['sh', str(self.repo / 'scripts/test-c-api.sh')], cwd=self.root,
                              env=environment, text=True, capture_output=True)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text(encoding='utf-8').splitlines()]

    def assert_current_consumers(self, result, library):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count('selected=73'), 3, result.stdout)
        self.assertNotIn('selected=12', result.stdout)
        commands = self.commands()
        self.assertEqual(commands[0], ['cargo', 'build', '-p', 'openjoc-capi', '--message-format=json-render-diagnostics'])
        c_commands = [command for command in commands if command[0] == 'cc']
        self.assertEqual(len(c_commands), 3)
        for command in c_commands:
            self.assertIn(str(library), command)
        self.assertEqual(sum(command[0] == 'c++' for command in commands), 1)

    def test_default_artifact_and_cached_build_are_selected(self):
        library = self.static_library('target/debug', 73)
        self.assert_current_consumers(self.run_script([self.artifact(library, fresh=True)]), library)

    def test_custom_cargo_layouts_are_selected(self):
        for layout in ('custom/release', 'target/host-triple/debug', 'custom 空 格/debug'):
            with self.subTest(layout=layout):
                library = self.static_library(layout, 73)
                self.environment['CARGO_TARGET_DIR'] = str(library.parent.parent)
                self.log.unlink(missing_ok=True)
                self.assert_current_consumers(self.run_script([self.artifact(library)]), library)

    def test_stale_default_library_is_never_linked(self):
        stale = self.static_library('target/debug', 12)
        previous = stale.read_bytes()
        current = self.static_library('configured-output/debug', 73)
        self.assert_current_consumers(self.run_script([self.artifact(current)]), current)
        self.assertEqual(stale.read_bytes(), previous)

    def test_unrelated_artifacts_and_build_status_are_ignored(self):
        current = self.static_library('configured-output/debug', 73)
        other = self.static_library('dependency-output/debug', 12)
        self.assert_current_consumers(self.run_script([
            self.artifact(other, name='dependency'), {'reason': 'build-script-executed', 'package_id': 'dependency'},
            self.artifact(current), {'reason': 'build-finished', 'success': True},
        ]), current)

    def test_missing_ambiguous_or_absent_staticlib_stops_before_compilation(self):
        current = self.static_library('configured-output/debug', 73)
        missing = current.with_name('missing.a')
        no_staticlib = self.artifact(current)
        no_staticlib['filenames'] = [str(current.with_suffix('.rlib'))]
        for messages in ([], [self.artifact(current), self.artifact(missing)], [self.artifact(missing)], [no_staticlib]):
            with self.subTest(messages=messages):
                self.log.unlink(missing_ok=True)
                result = self.run_script(messages)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.commands()), 1, result.stdout + result.stderr)

    def test_build_failure_never_uses_an_existing_library(self):
        library = self.static_library('target/debug', 12)
        result = self.run_script([self.artifact(library)], exit_code=42)
        self.assertEqual(result.returncode, 42, result.stdout + result.stderr)
        self.assertEqual(len(self.commands()), 1)


    def test_compiler_failures_propagate(self):
        library = self.static_library('configured-output/debug', 73)
        for compiler in ('cc', 'c++'):
            with self.subTest(compiler=compiler):
                self.environment['TEST_FAIL_COMPILER'] = compiler
                self.log.unlink(missing_ok=True)
                result = self.run_script([self.artifact(library)])
                self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
                self.assertEqual(len(self.commands()), 2 if compiler == 'cc' else 3)

    def test_consumer_failure_propagates(self):
        library = self.static_library('configured-output/debug', 73)
        self.environment['TEST_FAIL_CONSUMER'] = '1'
        result = self.run_script([self.artifact(library)])
        self.assertEqual(result.returncode, 29, result.stdout + result.stderr)
        self.assertEqual(len(self.commands()), 2)

    def test_fixture_failure_stops_before_legacy_consumer(self):
        library = self.static_library('configured-output/debug', 73)
        self.environment['TEST_FAIL_FIXTURES'] = '1'
        result = self.run_script([self.artifact(library)])
        self.assertEqual(result.returncode, 31, result.stdout + result.stderr)
        self.assertEqual(len(self.commands()), 4)
        self.assertEqual(result.stdout.count('selected=73'), 2)


if __name__ == '__main__':
    unittest.main()
