from __future__ import annotations

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
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
            pkg_environment = {key: value for key, value in os.environ.items()
                               if not key.startswith('PKG_CONFIG')}
            pkg_environment['PKG_CONFIG_LIBDIR'] = str(pc.parent)
            result = subprocess.run(['pkg-config', '--modversion', 'openjoc'], env=pkg_environment, text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout.strip(), '9.8.7')
            subprocess.run(['pkg-config', '--atleast-version=9.8.7', 'openjoc'], env=pkg_environment, check=True)
            flags = subprocess.check_output(
                ['pkg-config', '--cflags', '--libs', 'openjoc'],
                env=pkg_environment, text=True,
            )
            self.assertEqual(shlex.split(flags), [
                f'-I{prefix}/include', f'-L{prefix}/lib', '-lopenjoc_capi',
            ])

    @unittest.skipUnless(
        sys.platform.startswith('linux') and all(shutil.which(tool) for tool in
            ('python3', 'pkg-config', 'cc', 'ar')),
        'requires Linux C compiler, archiver, Python and pkg-config',
    )
    def test_staged_pkgconfig_compiles_and_runs_with_spaced_prefixes(self):
        # Real staging/pkg-config/compiler boundaries with a small C fixture.
        # Cargo is stubbed; this does not build or validate the OpenJOC ABI.
        with tempfile.TemporaryDirectory(prefix='openjoc-stage-consumer-') as tmp:
            root = Path(tmp)
            script = root / 'integrations/ffmpeg/native/stage-openjoc.sh'
            script.parent.mkdir(parents=True)
            shutil.copy2(ROOT / 'integrations/ffmpeg/native/stage-openjoc.sh', script)
            capi = root / 'crates/openjoc-capi'
            (capi / 'include').mkdir(parents=True)
            shutil.copy2(ROOT / 'crates/openjoc-capi/openjoc.pc.in', capi / 'openjoc.pc.in')
            (capi / 'include/openjoc.h').write_text('int openjoc_staging_probe(void);\n')
            library_source = root / 'library.c'
            library_source.write_text('int openjoc_staging_probe(void) { return 37; }\n')
            target = root / 'target/release'
            target.mkdir(parents=True)
            subprocess.run(['cc', '-shared', '-fPIC', str(library_source), '-o',
                            str(target / 'libopenjoc_capi.so')], check=True)
            subprocess.run(['cc', '-c', str(library_source), '-o', str(root / 'library.o')], check=True)
            subprocess.run(['ar', 'rcs', str(target / 'libopenjoc_capi.a'),
                            str(root / 'library.o')], check=True)
            consumer = root / 'consumer.c'
            consumer.write_text('#include <openjoc.h>\nint main(void) { return openjoc_staging_probe() == 37 ? 0 : 1; }\n')
            tools = root / 'tools'
            tools.mkdir()
            (tools / 'cargo').write_text(
                '#!/bin/sh\nif [ "$1" = metadata ]; then\n'
                "cat <<'EOF'\n"
                '{"packages":[{"name":"openjoc-capi","version":"9.8.7"}]}\n'
                'EOF\nfi\n'
            )
            (tools / 'uname').write_text('#!/bin/sh\nprintf "Linux\\n"\n')
            for tool in tools.iterdir():
                tool.chmod(0o755)
            environment = {key: value for key, value in os.environ.items()
                           if not key.startswith('PKG_CONFIG')}
            environment['PATH'] = f'{tools}:{environment["PATH"]}'
            for name in ('plain', 'prefix with spaces', 'prefix with & and | characters',
                         'prefix (release)'):
                with self.subTest(prefix=name):
                    prefix = root / name
                    subprocess.run(['sh', str(script), str(prefix)], env=environment, check=True)
                    flags = subprocess.check_output(
                        ['pkg-config', '--cflags', '--libs', 'openjoc'],
                        env={**environment, 'PKG_CONFIG_LIBDIR': str(prefix / 'lib/pkgconfig')},
                        text=True,
                    )
                    arguments = shlex.split(flags)
                    self.assertEqual(arguments, [f'-I{prefix}/include', f'-L{prefix}/lib',
                                                 '-lopenjoc_capi'])
                    executable = prefix / 'consumer'
                    subprocess.run(['cc', str(consumer), *arguments, '-o', str(executable)], check=True)
                    subprocess.run([str(executable)], env={**environment,
                                   'LD_LIBRARY_PATH': str(prefix / 'lib')}, check=True)


if __name__ == '__main__':
    unittest.main()
