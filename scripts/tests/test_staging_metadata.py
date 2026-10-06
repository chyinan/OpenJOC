from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.name != 'nt' and shutil.which('sh'), 'requires POSIX shell')
class StagingContractTests(unittest.TestCase):
    def test_release_verifier_invokes_path_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix='OpenJOC Release ') as tmp:
            root = Path(tmp)
            (root / 'bin').mkdir()
            executable = root / 'bin/openjoc'
            executable.write_text('#!/bin/sh\nprintf "OpenJOC test-version\\n"\n')
            executable.chmod(0o755)
            line = next(line for line in (ROOT / 'scripts/verify-release-bundle.sh').read_text().splitlines()
                        if line.startswith('help_output='))
            result = subprocess.run(['sh', '-ec', line + '\nprintf "%s" "$help_output"'],
                                    env={**os.environ, 'bundle_root': str(root)},
                                    text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout, 'OpenJOC test-version')

    @unittest.skipUnless(shutil.which('python3') and shutil.which('pkg-config'), 'requires Python and pkg-config')
    def test_stage_uses_resolved_cargo_version(self):
        with tempfile.TemporaryDirectory(prefix='openjoc stage ') as tmp:
            root = Path(tmp)
            script = root / 'integrations/ffmpeg/native/stage-openjoc.sh'
            script.parent.mkdir(parents=True)
            shutil.copy2(ROOT / 'integrations/ffmpeg/native/stage-openjoc.sh', script)
            template = root / 'crates/openjoc-capi/openjoc.pc.in'
            template.parent.mkdir(parents=True)
            shutil.copy2(ROOT / 'crates/openjoc-capi/openjoc.pc.in', template)
            (template.parent / 'include').mkdir()
            (template.parent / 'include/openjoc.h').touch()
            target = root / 'target/release'
            target.mkdir(parents=True)
            for name in ('libopenjoc_capi.a', 'libopenjoc_capi.so'):
                (target / name).touch()
            tools = root / 'tools'
            tools.mkdir()
            metadata = json.dumps({'packages': [{'name': 'openjoc-capi', 'version': '9.8.7'}]})
            (tools / 'cargo').write_text('#!/bin/sh\nif [ "$1" = metadata ]; then\ncat <<\'EOF\'\n' + metadata + '\nEOF\nfi\n')
            (tools / 'uname').write_text('#!/bin/sh\necho Linux\n')
            for tool in tools.iterdir():
                tool.chmod(0o755)
            prefix = root / 'prefix with & and | characters'
            subprocess.run(['sh', str(script), str(prefix)],
                           env={**os.environ, 'PATH': f'{tools}:{os.environ["PATH"]}'}, check=True)
            pc = prefix / 'lib/pkgconfig/openjoc.pc'
            self.assertIn(f'prefix={prefix}', pc.read_text())
            result = subprocess.run(['pkg-config', '--modversion', 'openjoc'], env={**os.environ, 'PKG_CONFIG_PATH': str(pc.parent)}, text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout.strip(), '9.8.7')
            subprocess.run(['pkg-config', '--atleast-version=9.8.7', 'openjoc'], env={**os.environ, 'PKG_CONFIG_PATH': str(pc.parent)}, check=True)


if __name__ == '__main__':
    unittest.main()
