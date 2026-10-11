# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

"""Exercise shipped preflight functions without real registry access or registration."""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import re
import subprocess
import tempfile
import unittest

WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = WORKSPACE / 'packaging/windows-lav-full/scripts/manual-registration.ps1'
HARNESS = WORKSPACE / 'scripts/tests/test_lav_registration_preflight.ps1'
ORIGINAL_REGFIX_SHA256 = '186dde96d33112c6064ba81bcb88a6170e2ffd2dda2e9ce644b7ef006bce18d2'


def function_text(text: str, name: str) -> str:
    matches = re.findall(rf'(?ms)^function {re.escape(name)}\b.*?^}}\n', text)
    if len(matches) != 1:
        raise AssertionError(f'Expected exactly one top-level function {name}')
    return matches[0]


def powershell_51_environment(system_root: pathlib.PurePath, inherited: dict[str, str]) -> dict[str, str]:
    # Python launched by pwsh inherits its PowerShell 7 module search path.
    # Give only this Windows PowerShell child its own built-in module directory;
    # never change the parent environment or any machine/user configuration.
    environment = {key: value for key, value in inherited.items() if key.casefold() != 'psmodulepath'}
    environment['PSModulePath'] = str(system_root / 'System32/WindowsPowerShell/v1.0/Modules')
    return environment


class LavRegistrationPreflightTests(unittest.TestCase):
    def test_extraction_preserves_every_original_production_statement(self) -> None:
        text = SCRIPT.read_text(encoding='utf-8')
        inventory = function_text(text, 'Assert-LavPackageInventory')
        adapter = function_text(text, 'Open-LavRegistrationBaseKey')
        self.assertEqual(adapter,
            'function Open-LavRegistrationBaseKey {\n'
            '    param([Microsoft.Win32.RegistryHive]$Hive)\n'
            '    return [Microsoft.Win32.RegistryKey]::OpenBaseKey($Hive, [Microsoft.Win32.RegistryView]::Registry64)\n'
            '}\n')
        prefix = "function Assert-LavPackageInventory {\n    param([string]$Root)\n    $runtime = Join-Path $Root 'runtime'\n"
        self.assertTrue(inventory.startswith(prefix))
        body = inventory[len(prefix):-2]
        filters = "    $filters = @('LAVAudio.ax', 'LAVVideo.ax', 'LAVSplitter.ax')\n"
        body = body.replace('    foreach ($name in $profile.required_runtime_files)',
                            filters + '    foreach ($name in $profile.required_runtime_files)', 1)
        restored = text.replace(inventory, '', 1).replace(adapter, '', 1)
        restored = restored.replace('    Assert-LavPackageInventory -Root $root\n' + filters, body, 1)
        restored = restored.replace('$base = Open-LavRegistrationBaseKey -Hive $hive',
            '$base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, [Microsoft.Win32.RegistryView]::Registry64)', 1)
        # Reversing only the approved extraction must reproduce the previously
        # vetted complete script, including production calls and failure order.
        self.assertEqual(hashlib.sha256(restored.encode()).hexdigest(), ORIGINAL_REGFIX_SHA256)

    def test_process_helper_is_byte_unchanged_from_verified_regfix(self) -> None:
        helper = function_text(SCRIPT.read_text(encoding='utf-8'), 'Invoke-LavRegistrationProcess')
        self.assertEqual(hashlib.sha256(helper.encode()).hexdigest(),
                         'dcac4b38cbd2960ad4bc8dfae5c6d2b948ca7581b83af6ff8641f0020c26902e')

    def test_new_harness_extracts_unmodified_functions_and_never_loads_adapter(self) -> None:
        text = HARNESS.read_text(encoding='utf-8')
        self.assertIn("foreach ($functionName in @('Assert-LavPackageInventory','Assert-OwnedPaths'))", text)
        self.assertIn('. ([scriptblock]::Create($definitions[0].Extent.Text))', text)
        self.assertNotIn('::OpenBaseKey(', text)
        self.assertNotIn('regsvr32.exe', text)
        self.assertNotRegex(text, r'(?m)^\s*(?:&|\.)\s+\$RegistrationScript\b')
        self.assertIn('function Open-LavRegistrationBaseKey {', text)
        self.assertIn("Assert-Equal $script:NativeCalls 0", text)

    def test_windows_child_uses_only_its_builtin_modules_without_mutating_parent(self) -> None:
        inherited = {'PSModulePath': 'PowerShell-7-modules', 'PSMODULEPATH': 'Other-modules', 'KEEP': 'unchanged'}
        original = inherited.copy()
        environment = powershell_51_environment(pathlib.PureWindowsPath('C:/Windows'), inherited)
        self.assertEqual(inherited, original)
        self.assertEqual(environment, {
            'KEEP': 'unchanged',
            'PSModulePath': r'C:\Windows\System32\WindowsPowerShell\v1.0\Modules',
        })

    @unittest.skipUnless(os.name == 'nt', 'Windows PowerShell 5.1 is required')
    def test_real_inventory_and_ownership_functions_with_isolated_fixtures(self) -> None:
        system_root = pathlib.Path(os.environ['SystemRoot'])
        # Sysnative selects a 64-bit host even from a 32-bit Python process.
        system = 'Sysnative' if os.environ.get('PROCESSOR_ARCHITEW6432') else 'System32'
        powershell = system_root / system / 'WindowsPowerShell/v1.0/powershell.exe'
        self.assertTrue(powershell.is_file(), 'Windows PowerShell 5.1 must be available on Windows')
        with tempfile.TemporaryDirectory(prefix='lav-preflight-') as temporary:
            scratch = pathlib.Path(temporary).resolve()
            receipt = scratch / 'receipt.json'
            result = subprocess.run([
                str(powershell), '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', str(HARNESS), '-RegistrationScript', str(SCRIPT),
                '-ScratchRoot', str(scratch), '-ReceiptPath', str(receipt),
            ], capture_output=True, text=True, timeout=120,
                env=powershell_51_environment(system_root, dict(os.environ)))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            data = json.loads(receipt.read_text(encoding='utf-8'))
            self.assertEqual((data['status'], data['total'], data['passed'], data['failed']),
                             ('passed', 51, 51, 0))
            self.assertEqual(sum(case['suite'] == 'inventory' for case in data['results']), 14)
            self.assertEqual(sum(case['suite'] == 'ownership' for case in data['results']), 37)
            self.assertEqual(len({case['name'] for case in data['results']}), 51)
            self.assertTrue(all(case['status'] == 'passed' for case in data['results']))
            self.assertTrue(data['powershell_version'].startswith('5.1.'))
            self.assertIs(data['process_is_64bit'], True)
            self.assertIs(data['real_registry_access'], False)
            self.assertIs(data['real_registration_attempted'], False)
            self.assertEqual(data['native_calls'], 0)
            self.assertEqual(data['registration_script_sha256'].lower(), hashlib.sha256(SCRIPT.read_bytes()).hexdigest())
            print(result.stdout.strip())


if __name__ == '__main__':
    unittest.main()
