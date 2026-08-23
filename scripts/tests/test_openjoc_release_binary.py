from __future__ import annotations

# pattern: Imperative Shell

import os
import re
import subprocess
import unittest
from pathlib import Path


EXPORT_LINE = re.compile(
    r"^\s*\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+"
    r"(openjoc_[A-Za-z0-9_]+)(?:\s+=\s+openjoc_[A-Za-z0-9_]+)?\s*$"
)

ABI_1_4_ADDITIONAL_EXPORTS = {"openjoc_decoder_config_init_v1_4"}


def exports(path: Path, dumpbin: Path) -> set[str]:
    output = subprocess.run(
        [str(dumpbin), "/nologo", "/exports", str(path)],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    ).stdout
    return {
        match.group(1)
        for line in output.splitlines()
        if (match := EXPORT_LINE.match(line))
    }


class OpenJocReleaseBinaryTests(unittest.TestCase):
    def paths(self) -> tuple[Path, Path, Path]:
        old = os.environ.get("OPENJOC_REFERENCE_CAPI")
        new = os.environ.get("OPENJOC_REBUILT_CAPI")
        dumpbin = os.environ.get("OPENJOC_DUMPBIN")
        if not old or not new or not dumpbin:
            raise unittest.SkipTest("reference, rebuilt DLL, and dumpbin paths are required")
        return Path(old), Path(new), Path(dumpbin)

    def test_c_abi_exports_are_unchanged(self) -> None:
        old, new, dumpbin = self.paths()
        previous_abi_exports = exports(old, dumpbin)
        final_abi_exports = exports(new, dumpbin)
        self.assertTrue(previous_abi_exports <= final_abi_exports)
        self.assertEqual(
            final_abi_exports,
            previous_abi_exports | ABI_1_4_ADDITIONAL_EXPORTS,
        )
        self.assertIn("openjoc_decoder_config_init_v1_4", final_abi_exports)

    def test_private_source_prefixes_are_absent_and_generic_prefix_is_present(self) -> None:
        _, new, _ = self.paths()
        data = new.read_bytes()
        private_markers = (
            b"C:\\Users\\chyin",
            "C:\\Users\\chyin".encode("utf-16-le"),
            b"D:\\Program\\OpenJOC",
            "D:\\Program\\OpenJOC".encode("utf-16-le"),
        )
        for marker in private_markers:
            self.assertNotIn(marker, data)
        self.assertIn(b"/openjoc", data)


if __name__ == "__main__":
    unittest.main()
