from __future__ import annotations

import argparse
import importlib.util
import os
from pathlib import Path, PureWindowsPath
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


class MacOSRelocationFixtures(unittest.TestCase):
    def test_otool_identity_is_not_treated_as_dependency_fixture(self):
        outputs = ['libsample.dylib:\n\t/builder/libsample.dylib',
                   'libsample.dylib:\n\t/builder/libsample.dylib (compatibility version 1.0.0, current version 1.0.0)\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1.0.0)']
        with patch.object(PACKAGE._MACOS, 'run', side_effect=outputs):
            self.assertEqual(PACKAGE._MACOS.dependencies(Path('libsample.dylib')), ['/usr/lib/libSystem.B.dylib'])

    def test_final_signing_follows_binary_mutations_before_checksums_fixture(self):
        events = []
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = root / 'stage'
            stage.mkdir()
            with patch.object(PACKAGE, 'sanitize_private', side_effect=lambda _: events.append('sanitize')), patch.object(PACKAGE._MACOS, 'verify', side_effect=lambda _: events.append('verify')), patch.object(PACKAGE._MACOS, 'sign', side_effect=lambda _: events.append('sign')), patch.object(PACKAGE, 'write_sha256sums', side_effect=lambda _: events.append('checksum')), patch.object(PACKAGE, 'version', return_value='fixture'), patch.object(PACKAGE, 'run', return_value='fixture'):
                PACKAGE.finish_package(stage, root, 'fixture', 'macos-arm64', 'ffmpeg')
        self.assertEqual(events, ['sanitize', 'verify', 'sign', 'checksum'])

    def test_verifier_does_not_inject_macos_loader_paths(self):
        completed = subprocess.CompletedProcess([], 0, stdout=b'fixture')
        with patch.object(VERIFY.subprocess, 'run', return_value=completed) as run:
            VERIFY.run_binary(Path('/bundle/bin/openjoc-ffmpeg'), Path('/bundle'), 'macos-arm64')
            self.assertNotIn('LD_LIBRARY_PATH', run.call_args.kwargs['env'])
            self.assertNotIn('DYLD_LIBRARY_PATH', run.call_args.kwargs['env'])

    def test_rejects_unbundled_dependency_fixture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with patch.object(PACKAGE._MACOS, 'dependencies', return_value=['/opt/local/libmissing.dylib']):
                with self.assertRaisesRegex(RuntimeError, 'unbundled Mach-O dependency'):
                    PACKAGE._MACOS.relocate(root)

    def test_windows_host_paths_emit_posix_macho_references_fixture(self):
        macho = PACKAGE._MACOS
        root = PureWindowsPath('C:/bundle with spaces')
        owners = [root / 'bin/openjoc-ffmpeg', root / 'bin/openjoc-ffprobe',
                  root / 'lib/libsample.dylib']
        with patch.object(macho, 'images', return_value=owners), patch.object(macho, 'dependencies', return_value=['/builder/libsample.dylib']), patch.object(macho, 'rpaths', return_value=[]), patch.object(macho, 'verify'), patch.object(macho, 'run') as run:
            macho.relocate(root)
        arguments = [call.args for call in run.call_args_list]
        self.assertTrue(any('@loader_path/../lib/libsample.dylib' in args for args in arguments))
        self.assertTrue(any('@loader_path/libsample.dylib' in args for args in arguments))
        self.assertFalse(any('\\' in arg for args in arguments for arg in args if arg.startswith('@loader_path/')))

    def test_relocation_preserves_system_dependencies_fixture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'lib').mkdir()
            (root / 'lib/libsample.dylib').touch()
            macho = PACKAGE._MACOS
            with patch.object(macho, 'dependencies', return_value=['/usr/lib/libSystem.B.dylib', '/builder/libsample.dylib']), patch.object(macho, 'rpaths', return_value=['/builder/lib']), patch.object(macho, 'verify'), patch.object(macho, 'run') as run:
                macho.relocate(root)
            arguments = [call.args for call in run.call_args_list]
            self.assertTrue(any('@loader_path/../lib/libsample.dylib' in args for args in arguments))
            self.assertTrue(any('@loader_path/libsample.dylib' in args for args in arguments))
            self.assertFalse(any('/usr/lib/libSystem.B.dylib' in args for args in arguments))
            self.assertTrue(all('-delete_rpath' in args for args in arguments))


@unittest.skipUnless(sys.platform == 'darwin' and shutil.which('cc'), 'requires native macOS compiler and loader')
class NativeMacOSRelocationTests(unittest.TestCase):
    def test_archive_runs_without_build_prefix_or_loader_environment(self):
        with tempfile.TemporaryDirectory(prefix='openjoc native relocation ') as tmp:
            base = Path(tmp)
            prefix = base / 'builder prefix'
            (prefix / 'bin').mkdir(parents=True)
            (prefix / 'lib').mkdir()
            (base / 'dependency.c').write_text('const char build_marker[] = "/home/runner/native-fixture"; int sample(void) { return 73; }\n')
            (base / 'middle.c').write_text('int sample(void); int middle(void) { return sample(); }\n')
            (base / 'main.c').write_text('#include <stdio.h>\nint middle(void); int main(int argc, char **argv) { for (int i=1;i<argc;i++) puts(argv[i]); return middle(); }\n')
            library = prefix / 'lib/libsample.dylib'
            middle = prefix / 'lib/libmiddle.dylib'
            subprocess.run(['cc', '-dynamiclib', str(base / 'dependency.c'), '-Wl,-headerpad_max_install_names', '-install_name', str(library), '-o', str(library)], check=True)
            subprocess.run(['cc', '-dynamiclib', str(base / 'middle.c'), str(library), '-Wl,-headerpad_max_install_names', '-install_name', str(middle), '-o', str(middle)], check=True)
            subprocess.run(['cc', str(base / 'main.c'), str(middle), '-Wl,-headerpad_max_install_names', '-Wl,-rpath,' + str(prefix / 'lib'), '-o', str(prefix / 'bin/ffmpeg')], check=True)
            shutil.copy2(prefix / 'bin/ffmpeg', prefix / 'bin/ffprobe')
            self.assertIn(b'/home/runner/native-fixture', library.read_bytes())
            source = base / 'source'
            source.mkdir()
            (source / 'LICENSE').write_text('Native test fixture')
            args = argparse.Namespace(output=base / 'output', platform='macos-arm64',
                                      ffmpeg=prefix / 'bin/ffmpeg', ffprobe=prefix / 'bin/ffprobe',
                                      openjoc_prefix=None, ffmpeg_source=source,
                                      ffmpeg_revision='fixture', openjoc_patch_sha256='fixture')
            with patch.object(PACKAGE, 'version', return_value='0.18.0'), patch.object(PACKAGE, 'run', return_value='test-commit'):
                PACKAGE.package_ffmpeg(args)
            with tarfile.open(next(args.output.glob('*.tar.gz'))) as archive:
                archive.extractall(base / 'extracted with spaces')
            root = next((base / 'extracted with spaces').iterdir())
            shutil.rmtree(prefix)
            self.assertNotIn(b'/home/runner', (root / 'lib/libsample.dylib').read_bytes())
            VERIFY.verify_checksums(root)
            PACKAGE._MACOS.verify(root)
            for owner in PACKAGE._MACOS.images(root):
                PACKAGE._MACOS.run('codesign', '--verify', '--strict', str(owner))
            for name in ('openjoc-ffmpeg', 'openjoc-ffprobe'):
                status, output = VERIFY.run_binary(root / 'bin' / name, root, 'macos-arm64', ('a b', '*'), check=False)
                self.assertEqual((status, output), (73, 'a b\n*\n'))


if __name__ == '__main__':
    unittest.main()
