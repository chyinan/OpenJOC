# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts" / "windows_multichannel_qa" / "Run-OpenJocEndpointQa.ps1"
WRAPPER = ROOT / "scripts" / "windows_multichannel_qa" / "Run-OpenJocEndpointQa.cmd"
README = ROOT / "scripts" / "windows_multichannel_qa" / "README.md"
BUILDER = ROOT / "scripts" / "build_lav_endpoint_qa_package.ps1"


class LavEndpointQaPackageTests(unittest.TestCase):
    def test_runner_is_exact_read_only_endpoint_qa(self) -> None:
        text = RUNNER.read_text(encoding="utf-8")

        for required in (
            "--list-audio-renderers",
            "--inspect-audio-endpoint",
            "--native-renderer-probe",
            "RendererMoniker",
            "EndpointId",
            "EndpointKind",
            "0..6",
            "joc.lifecycle.ec3",
            "joc.lifecycle.mp4",
            "proposalCount",
            "fallbackProposals",
            "connectDirectHresult",
            "requestedMediaType",
            "acceptedRendererMediaType",
            "sampleDelivery",
            "report.json",
            "VIRTUAL_WINDOWS_ENDPOINT_VERIFIED",
            "WINDOWS_ENDPOINT_SAMPLE_DELIVERY_VERIFIED",
        ):
            self.assertIn(required, text)
        for forbidden in (
            "SetDefault",
            "SetValue",
            "PropertyStore::Commit",
            "CABLE In",
            "Realtek",
            "Bass Management",
            "fallback mask",
        ):
            self.assertNotIn(forbidden, text)

    def test_runner_uses_private_runtime_and_fresh_manifest(self) -> None:
        text = RUNNER.read_text(encoding="utf-8")

        self.assertIn("PACKAGE_SHA256.tsv", text)
        self.assertIn("Get-FileHash", text)
        self.assertIn("report-runtime", text)
        self.assertIn("--write-manifest", text)
        self.assertIn("OpenJocRuntimeIdentity.tsv", text)
        self.assertIn("IsReadOnly", text)
        self.assertNotIn("regsvr32", text.lower())
        self.assertIn("${LASTEXITCODE}:", text)

    def test_builder_emits_self_contained_zip_without_build_tools(self) -> None:
        text = BUILDER.read_text(encoding="utf-8")

        for required in (
            "RuntimeDirectory",
            "FixtureDirectory",
            "OpenJocDirectShowNegotiationSmoke.exe",
            "LAVAudio.ax",
            "LAVSplitter.ax",
            "openjoc_capi.dll",
            "joc.lifecycle.ec3",
            "joc.lifecycle.mp4",
            "PACKAGE_SHA256.tsv",
            "Compress-Archive",
        ):
            self.assertIn(required, text)
        self.assertNotIn("msbuild", text.lower())
        self.assertNotIn("cl.exe", text.lower())

    def test_package_has_cmd_entrypoint_and_scope_documentation(self) -> None:
        wrapper = WRAPPER.read_text(encoding="utf-8")
        readme = README.read_text(encoding="utf-8")

        self.assertIn("powershell.exe", wrapper.lower())
        for required in (
            "VIRTUAL_WINDOWS_ENDPOINT_VERIFIED",
            "REAL_ENDPOINT_VERIFIED",
            "PHYSICAL_MULTICHANNEL_HARDWARE_VERIFIED",
            "does not",
            "renderer moniker",
            "endpoint ID",
        ):
            self.assertIn(required, readme)


if __name__ == "__main__":
    unittest.main()
