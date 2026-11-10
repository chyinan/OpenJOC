from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "verify_ecosystem_gstreamer_dispatch", ROOT / "scripts/verify-ecosystem-package.py"
)
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class GStreamerDispatchTests(unittest.TestCase):
    def make_archive(self, base: Path, platform: str, *, corrupt: bool = False,
                     missing: str | None = None) -> Path:
        root = base / "package with spaces"
        root.mkdir()
        files = {
            "LICENSE": b"Synthetic dispatch fixture\n",
            "THIRD_PARTY_NOTICES.md": b"Synthetic dispatch fixture\n",
            "DEPENDENCIES": b"{}\n",
            "BUILD_INFO": json.dumps({"package_kind": "gstreamer-plugin", "platform": platform}).encode(),
            "QUICKSTART.md": b"Synthetic dispatch fixture\n",
            "lib/gstreamer-1.0/libgstopenjoc.so": b"Synthetic plugin; native execution is mocked.\n",
        }
        for name, data in files.items():
            if name == missing:
                continue
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        records = [f"{hashlib.sha256(data).hexdigest()}  {name}"
                   for name, data in sorted(files.items()) if name != missing]
        (root / "SHA256SUMS").write_text("\n".join(records) + "\n", encoding="utf-8")
        if corrupt:
            (root / "LICENSE").write_bytes(b"changed after checksum\n")
        archive = base / ("package.zip" if platform == "windows-x64" else "package.tar.gz")
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive, "w") as output:
                for path in root.rglob("*"):
                    if path.is_file():
                        output.write(path, path.relative_to(base))
        else:
            with tarfile.open(archive, "w:gz") as output:
                output.add(root, arcname=root.name)
        return archive

    def run_main(self, archive: Path, platform: str, *, run_gstreamer: bool = True) -> str:
        arguments = ["verify-ecosystem-package.py", "--archive", str(archive),
                     "--kind", "gstreamer-plugin", "--platform", platform]
        if run_gstreamer:
            arguments.append("--run-gstreamer")
        output = io.StringIO()
        with patch.object(sys, "argv", arguments), contextlib.redirect_stdout(output):
            self.assertEqual(VERIFY.main(), 0)
        return output.getvalue()

    def test_requested_runtime_check_runs_both_elements_on_every_platform(self):
        for platform in ("linux-x86_64", "macos-arm64", "windows-x64"):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary)
                archive = self.make_archive(base, platform)
                tool = str(base / "tools with spaces" / "gst-inspect-1.0")
                poisoned = {"GST_PLUGIN_PATH": "inherited-plugins", "GST_REGISTRY_1_0": "inherited-registry",
                            "LD_LIBRARY_PATH": "inherited-loader", "DYLD_LIBRARY_PATH": "inherited-loader"}
                with patch.dict(os.environ, poisoned), patch.object(VERIFY.shutil, "which", return_value=tool) as which, patch.object(VERIFY.subprocess, "run") as run:
                    output = self.run_main(archive, platform)
                which.assert_called_once_with("gst-inspect-1.0")
                self.assertEqual([call.args[0] for call in run.call_args_list],
                                 [[tool, "openjocclassify"], [tool, "openjocdec"]])
                for call in run.call_args_list:
                    self.assertIs(call.kwargs["check"], True)
                    environment = call.kwargs["env"]
                    plugin_path = Path(environment["GST_PLUGIN_PATH"])
                    extracted = plugin_path.parent.parent
                    self.assertEqual(plugin_path, extracted / "lib/gstreamer-1.0")
                    self.assertEqual(extracted.name, "package with spaces")
                    self.assertEqual(environment["GST_REGISTRY_1_0"], str(extracted.parent / "gst-registry.bin"))
                    self.assertIn(str(Path(tool).resolve().parent), environment["PATH"])
                    self.assertFalse(any(value in environment.values() for value in poisoned.values()))
                self.assertIn("GStreamer verifier isolation:", output)
                self.assertIn("ecosystem package PASS:", output)

    def test_missing_inspector_rejects_requested_runtime_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = self.make_archive(Path(temporary), "linux-x86_64")
            with patch.object(VERIFY.shutil, "which", return_value=None), patch.object(VERIFY.subprocess, "run") as run:
                with self.assertRaisesRegex(SystemExit, "requires gst-inspect-1.0"):
                    self.run_main(archive, "linux-x86_64")
                run.assert_not_called()

    def test_element_failure_is_propagated_without_success(self):
        for failed_element, expected_calls in (("openjocclassify", 1), ("openjocdec", 2)):
            with self.subTest(element=failed_element), tempfile.TemporaryDirectory() as temporary:
                archive = self.make_archive(Path(temporary), "linux-x86_64")
                def inspect(arguments, **kwargs):
                    if arguments[1] == failed_element:
                        raise subprocess.CalledProcessError(17, arguments)
                    return subprocess.CompletedProcess(arguments, 0)
                with patch.object(VERIFY.shutil, "which", return_value="gst-inspect-1.0"), patch.object(VERIFY.subprocess, "run", side_effect=inspect) as run:
                    with self.assertRaises(subprocess.CalledProcessError) as raised:
                        self.run_main(archive, "linux-x86_64")
                    self.assertEqual(raised.exception.cmd[1], failed_element)
                    self.assertEqual(run.call_count, expected_calls)

    def test_runtime_check_remains_explicitly_opt_in(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = self.make_archive(Path(temporary), "linux-x86_64")
            with patch.object(VERIFY.shutil, "which") as which, patch.object(VERIFY.subprocess, "run") as run:
                output = self.run_main(archive, "linux-x86_64", run_gstreamer=False)
                which.assert_not_called()
                run.assert_not_called()
            self.assertIn("ecosystem package PASS:", output)

    def test_checksums_are_verified_before_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = self.make_archive(Path(temporary), "linux-x86_64", corrupt=True)
            with patch.object(VERIFY.shutil, "which") as which:
                with self.assertRaisesRegex(SystemExit, "checksum mismatch"):
                    self.run_main(archive, "linux-x86_64")
                which.assert_not_called()

    def test_required_metadata_is_verified_before_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = self.make_archive(Path(temporary), "linux-x86_64", missing="QUICKSTART.md")
            with patch.object(VERIFY.shutil, "which") as which:
                with self.assertRaisesRegex(SystemExit, "required package file is missing: QUICKSTART.md"):
                    self.run_main(archive, "linux-x86_64")
                which.assert_not_called()


if __name__ == "__main__":
    unittest.main()
