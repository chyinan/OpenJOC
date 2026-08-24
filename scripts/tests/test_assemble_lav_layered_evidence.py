# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from assemble_lav_layered_evidence import (  # noqa: E402
    _attributes,
    parse_controlled_sink_text,
    parse_native_probe_text,
)


class AssembleLavLayeredEvidenceTests(unittest.TestCase):
    def test_attribute_parser_stops_before_keys_containing_digits(self) -> None:
        values = _attributes(
            r"fixture_path=D:\fixtures\sample.mp4 fixture_sha256=" + "a" * 64
        )

        self.assertEqual(values["fixture_path"], r"D:\fixtures\sample.mp4")
        self.assertEqual(values["fixture_sha256"], "a" * 64)

    def test_parses_exact_endpoint_delivery_witness(self) -> None:
        text = "\n".join(
            (
                "NATIVE_RENDERER_PROBE_V1",
                "result\tINITIAL_STREAM_OBSERVED",
                "renderer_moniker\t@device:cm:test",
                "fixture_path\tD:\\fixtures\\joc.lifecycle.ec3",
                "fixture_sha256\t" + "a" * 64,
                "policy\t4",
                "connect_attempted\t1",
                "proposal_count\t1",
                "fallback_proposals\t0",
                "requested_type\texact-type",
                "connect_direct_hr\t0x00000000",
                "type_observation\tpre_stream\toutput_exact=1\trenderer_input_exact=1\tpeer_equal=1\toutput_type=exact-type\trenderer_input_type=exact-type",
                "type_observation\tpost_stream\toutput_exact=1\trenderer_input_exact=1\tpeer_equal=1\toutput_type=exact-type\trenderer_input_type=exact-type",
                "operation\t1\tinitial_stream\tgraph_error_hr=0x00000000\tmidstream_last_buffer_duration=32\tclassifier_bytes=8192\tstream_bytes=524288\teos_complete=1",
            )
        )

        parsed = parse_native_probe_text(text)

        self.assertEqual(parsed["result"], "INITIAL_STREAM_OBSERVED")
        self.assertTrue(parsed["sample_delivery"])
        self.assertEqual(parsed["accepted_type"], "exact-type")
        self.assertEqual(parsed["fallback_proposals"], 0)

    def test_parses_exact_rejection_without_claiming_delivery(self) -> None:
        text = "\n".join(
            (
                "NATIVE_RENDERER_PROBE_V1",
                "result\tEXACT_REJECTION",
                "renderer_moniker\t@device:cm:test",
                "fixture_path\tD:\\fixtures\\joc.lifecycle.mp4",
                "fixture_sha256\t" + "b" * 64,
                "policy\t6",
                "connect_attempted\t1",
                "proposal_count\t1",
                "fallback_proposals\t0",
                "requested_type\texact-type",
                "connect_direct_hr\t0x8004025c",
                "type_observation\tpre_stream\toutput_exact=0\trenderer_input_exact=0",
                "type_observation\tpost_stream\toutput_exact=0\trenderer_input_exact=0",
                "operation\t1\tinitial_stream\tgraph_error_hr=0x00000000\tmidstream_last_buffer_duration=0\tclassifier_bytes=0\tstream_bytes=0\teos_complete=0",
            )
        )

        parsed = parse_native_probe_text(text)

        self.assertFalse(parsed["sample_delivery"])
        self.assertIsNone(parsed["accepted_type"])
        self.assertEqual(parsed["connect_direct_hr"], "0x8004025c")

    def test_controlled_sink_parser_requires_both_paths_for_all_layouts(self) -> None:
        line = (
            r"CONTROLLED_SINK_COMPLETE fixture_path=D:\fixtures\joc.fingerprint.ec3 "
            + "fixture_sha256=" + "a" * 64
            + " oracle_sha256=" + "a" * 64
            + " policy=Stereo channels=2 mask=0x00000003 channel_order=FL,FR "
            + "format_tag=0xfffe subtype=IEEE_FLOAT sample_rate=48000 bits=32 valid_bits=32 "
            + "block_align=8 avg_bytes_per_sec=384000 actual_frame_size=8 "
            + "checked_buffer_sizing=1 allocator_contract_valid=1 frame_aligned=1 "
            + "full_interleaved_oracle_equal=1 per_channel_oracle_equal=1 "
            + "per_channel_digests_pairwise_distinct=1 proposals=1 fallback_proposals=0 "
            + "type_mutations=0 eos=1 samples=2 bytes=12544"
        )

        with self.assertRaisesRegex(ValueError, "raw and MP4"):
            parse_controlled_sink_text(line)


if __name__ == "__main__":
    unittest.main()
