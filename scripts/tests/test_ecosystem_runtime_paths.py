from __future__ import annotations

import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name: str):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PLAYER = load('player-package')
PACKAGE = load('package-ecosystem')
VERIFY = load('verify-ecosystem-package')


class DependencyPathTests(unittest.TestCase):
    def test_ldd_preserves_spaces_and_parentheses(self):
        output = ('linux-vdso.so.1 (0x0123)\n'
                  'libsample.so => /opt/Player Build (release)/libsample.so (0x123abc)\n'
                  '/opt/Loader Build/ld-linux.so.2 (0x456def)\n')
        with patch.object(PLAYER, 'run', return_value=output):
            self.assertEqual(PLAYER.parse_ldd(Path('mpv')), [
                ('libsample.so', Path('/opt/Player Build (release)/libsample.so')),
                ('ld-linux.so.2', Path('/opt/Loader Build/ld-linux.so.2'))])

    def test_ldd_still_rejects_missing_dependencies(self):
        with patch.object(PLAYER, 'run', return_value='libsample.so => not found\n'):
            with self.assertRaisesRegex(RuntimeError, 'unresolved ELF dependency'):
                PLAYER.parse_ldd(Path('mpv'))

    def test_macho_preserves_spaces_fixture_only(self):
        output = ('mpv:\n\t/opt/Player Build/libsample.dylib '
                  '(compatibility version 1.0.0, current version 1.2.3)\n')
        with patch.object(PLAYER, 'run', return_value=output):
            self.assertEqual(PLAYER.parse_macho_dependencies(Path('mpv')),
                             ['/opt/Player Build/libsample.dylib'])


@unittest.skipUnless(shutil.which('bash') and os.name != 'nt', 'requires Bash on Unix')
class ActivationTests(unittest.TestCase):
    def test_packaged_activation_sources_from_another_directory(self):
        with tempfile.TemporaryDirectory(prefix='openjoc activation ') as tmp:
            base = Path(tmp)
            plugin = base / 'libgstopenjoc.so'
            plugin.write_bytes(b'activation test fixture')
            output = base / 'output'
            args = argparse.Namespace(output=output, platform='linux-x86_64',
                                      plugin=plugin, openjoc_library=None,
                                      gstreamer_baseline='test')
            with patch.object(PACKAGE, 'version', return_value='0.18.0'), patch.object(PACKAGE, 'run', return_value='test-commit'):
                PACKAGE.package_gstreamer(args)
            with tarfile.open(next(output.glob('*.tar.gz'))) as archive:
                archive.extractall(base / 'extracted')
            root = next((base / 'extracted').iterdir())
            script = root / 'activate.sh'
            result = subprocess.run(['bash', '-c', 'source "$1"; printf "%s" "$GST_PLUGIN_PATH"',
                                     '_', str(script)], cwd=base,
                                    env={**os.environ, 'GST_PLUGIN_PATH': '/previous/plugins'},
                                    text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout, f'{root}/lib/gstreamer-1.0:/previous/plugins')
            result = subprocess.run(['bash', '-c', 'unset GST_PLUGIN_PATH; source "$1"; printf "%s" "$GST_PLUGIN_PATH"',
                                     '_', str(script)], cwd=base, text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout, f'{root}/lib/gstreamer-1.0')
            result = subprocess.run([str(script)], cwd=base, text=True, capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn('Source this file in Bash', result.stderr)
            self.assertIn('source "/absolute/path', (root / 'QUICKSTART.md').read_text())


@unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('cc'), 'requires Linux C compiler')
class LinuxLauncherTests(unittest.TestCase):
    def test_real_packager_preserves_arguments_and_exit_status_without_loader_setup(self):
        with tempfile.TemporaryDirectory(prefix='openjoc runtime ') as tmp:
            base = Path(tmp)
            prefix = base / 'prefix'
            (prefix / 'bin').mkdir(parents=True)
            (prefix / 'lib').mkdir()
            (base / 'dependency.c').write_text('int sample(void) { return 73; }\n')
            (base / 'main.c').write_text('#include <stdio.h>\nint sample(void);\nint main(int argc, char **argv) { for (int i=1;i<argc;i++) puts(argv[i]); return sample(); }\n')
            subprocess.run(['cc', '-shared', '-fPIC', str(base / 'dependency.c'),
                            '-Wl,-soname,libopenjoc_test.so', '-o', str(prefix / 'lib/libopenjoc_test.so')], check=True)
            subprocess.run(['cc', str(base / 'main.c'), f'-L{prefix}/lib', '-lopenjoc_test',
                            '-o', str(prefix / 'bin/ffmpeg')], check=True)
            shutil.copyfile(prefix / 'bin/ffmpeg', prefix / 'bin/ffprobe')
            (prefix / 'bin/ffprobe').chmod(0o755)
            source = base / 'source'
            source.mkdir()
            (source / 'LICENSE').write_text('Test fixture')
            args = argparse.Namespace(output=base / 'output', platform='linux-x86_64',
                                      ffmpeg=prefix / 'bin/ffmpeg', ffprobe=prefix / 'bin/ffprobe',
                                      openjoc_prefix=None, ffmpeg_source=source,
                                      ffmpeg_revision='fixture', openjoc_patch_sha256='fixture')
            with patch.object(PACKAGE, 'version', return_value='0.18.0'), patch.object(PACKAGE, 'run', return_value='test-commit'):
                PACKAGE.package_ffmpeg(args)
            with tarfile.open(next(args.output.glob('*.tar.gz'))) as archive:
                archive.extractall(base / 'extracted')
            root = next((base / 'extracted').iterdir())
            shutil.rmtree(prefix)
            for name in ('openjoc-ffmpeg', 'openjoc-ffprobe'):
                result = subprocess.run([str(root / 'bin' / name), 'a b', '*', '--flag=1'],
                                        cwd=base, env={'PATH': '/usr/bin:/bin'}, text=True, capture_output=True)
                self.assertEqual(result.returncode, 73, result.stderr)
                self.assertEqual(result.stdout, 'a b\n*\n--flag=1\n')
                status, output = VERIFY.run_binary(root / 'bin' / name, root, 'linux-x86_64', ('a b',), check=False)
                self.assertEqual((status, output), (73, 'a b\n'))

    def test_verifier_does_not_inject_linux_loader_path(self):
        completed = subprocess.CompletedProcess([], 0, stdout=b'fixture')
        with patch.object(VERIFY.subprocess, 'run', return_value=completed) as run:
            VERIFY.run_binary(Path('/bundle/bin/openjoc-ffmpeg'), Path('/bundle'), 'linux-x86_64')
            self.assertNotIn('LD_LIBRARY_PATH', run.call_args.kwargs['env'])
            self.assertNotIn('DYLD_LIBRARY_PATH', run.call_args.kwargs['env'])


if __name__ == '__main__':
    unittest.main()
