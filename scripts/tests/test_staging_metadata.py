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


def write_cargo_stub(path: Path, *archive_paths: str):
    metadata = {"packages": [{"name": "openjoc-capi", "version": "9.8.7"}]}
    artifact = {
        "reason": "compiler-artifact",
        "target": {"name": "openjoc_capi", "crate_types": ["rlib", "staticlib", "cdylib"]},
        "filenames": list(archive_paths),
        "fresh": True,
    }
    path.write_text(
        "#!/bin/sh\n"
        "case \"$1\" in\n"
        "metadata) cat <<'METADATA'\n"
        + json.dumps(metadata) + "\nMETADATA\n;;\n"
        "build) cat <<'ARTIFACT'\n"
        + json.dumps(artifact) + "\nARTIFACT\n;;\n"
        "*) exit 2 ;;\n"
        "esac\n"
    )
    path.chmod(0o755)


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
            write_cargo_stub(tools / 'cargo', str(target / 'libopenjoc_capi.a'))
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
            write_cargo_stub(tools / 'cargo', str(target / 'libopenjoc_capi.a'))
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


    @unittest.skipUnless(shutil.which('python3'), 'requires Python')
    def test_stage_follows_the_build_artifact_and_rejects_missing_artifact(self):
        with tempfile.TemporaryDirectory(prefix='openjoc-stage-target-') as tmp:
            root = Path(tmp)
            script = root / 'integrations/ffmpeg/native/stage-openjoc.sh'
            capi = root / 'crates/openjoc-capi'
            script.parent.mkdir(parents=True)
            (capi / 'include').mkdir(parents=True)
            shutil.copy2(ROOT / 'integrations/ffmpeg/native/stage-openjoc.sh', script)
            shutil.copy2(ROOT / 'crates/openjoc-capi/openjoc.pc.in', capi / 'openjoc.pc.in')
            (capi / 'include/openjoc.h').touch()
            tools = root / 'tools'
            tools.mkdir()
            (tools / 'uname').write_text('#!/bin/sh\nprintf "%s\n" "$TEST_HOST_OS"\n')
            (tools / 'cygpath').write_text(
                '#!/bin/sh\n[ "$1" = "-u" ] && [ "$2" = "$TEST_NATIVE_ARCHIVE" ] || exit 2\n'
                'printf "%s\n" "$TEST_POSIX_ARCHIVE"\n'
            )
            for tool in tools.iterdir():
                tool.chmod(0o755)
            for label, host, flag in (
                ('directory', 'Linux', ''),
                ('implicit-target', 'Linux', ''),
                ('explicit-target', 'Linux', 'x86_64-unknown-linux-gnu'),
                ('mingw-gnu', 'MINGW64_NT-10.0', 'x86_64-pc-windows-gnu'),
            ):
                with self.subTest(label=label):
                    target = root / ('custom 输出 ' + label)
                    triple = flag or ('x86_64-unknown-linux-gnu' if label == 'implicit-target' else '')
                    current = target / triple / 'release'
                    stale = root / 'target' / flag / 'release'
                    outputs = {'lib/libopenjoc_capi.a': 'libopenjoc_capi.a'}
                    if host == 'Linux':
                        outputs['lib/libopenjoc_capi.so'] = 'libopenjoc_capi.so'
                    else:
                        outputs['bin/openjoc_capi.dll'] = 'openjoc_capi.dll'
                        outputs['lib/libopenjoc_capi.dll.a'] = 'openjoc_capi.dll.a'
                    for directory, marker in ((current, 'current'), (stale, 'stale')):
                        directory.mkdir(parents=True, exist_ok=True)
                        for filename in outputs.values():
                            (directory / filename).write_text(marker)
                    archive = current / 'libopenjoc_capi.a'
                    native = str(archive) if host == 'Linux' else r'C:\custom output\release\libopenjoc_capi.a'
                    write_cargo_stub(tools / 'cargo', native)
                    prefix = root / ('stage-' + label)
                    env = {**os.environ, 'PATH': f'{tools}:{os.environ["PATH"]}', 'TEST_HOST_OS': host,
                           'CARGO_TARGET_DIR': str(target), 'TEST_NATIVE_ARCHIVE': native,
                           'TEST_POSIX_ARCHIVE': str(archive)}
                    if label == 'implicit-target':
                        env['CARGO_BUILD_TARGET'] = triple
                    subprocess.run(['sh', str(script), str(prefix), *([flag] if flag else [])],
                                   env=env, check=True)
                    for staged in outputs:
                        self.assertEqual((prefix / staged).read_text(), 'current')
            write_cargo_stub(tools / 'cargo')
            prefix = root / 'no-artifact'
            result = subprocess.run(['sh', str(script), str(prefix)],
                                    env={**os.environ, 'PATH': f'{tools}:{os.environ["PATH"]}'},
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Cargo did not report exactly one OpenJOC C ABI static archive', result.stderr)
            self.assertFalse(prefix.exists())


if __name__ == '__main__':
    unittest.main()
