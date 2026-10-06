#!/usr/bin/env python3
"""Summarize paired timing repeats without making a performance claim.

The input is a `measurements.csv` emitted by `verify_pcm_bitexact.py`. Rows
with malformed/invalid paired timings, an explicit failed validity marker, or
inconsistent workload descriptors are excluded from the reported statistics.
The helper reports descriptive side-to-side deltas only; it does not infer
speedup or validate PCM exactness itself.
"""

from __future__ import annotations

import argparse
import csv
import math
from pathlib import Path
import statistics
import sys


VALID_MARKERS = {"1", "true", "yes", "valid", "pass", "passed"}
VALIDITY_COLUMNS = ("valid", "is_valid", "status", "gate_status", "validation_status")
SHAPE_COLUMNS = (
    "input_sha256",
    "mode",
    "layout",
    "input_access_units",
    "output_frames",
    "output_samples",
    "output_sample_count",
    "tail_samples",
    "channels",
    "channel_count",
    "config_fingerprint",
    "config_descriptor_hex",
    "latency_samples",
)
BINARY_HASH_COLUMNS = (
    "baseline_api_probe_binary_sha256",
    "candidate_api_probe_binary_sha256",
    "baseline_core_probe_binary_sha256",
    "candidate_core_probe_binary_sha256",
)


class AnalysisError(ValueError):
    """Raised when the input cannot support a paired timing summary."""


def _timing_metric(stem: str) -> bool:
    lower = stem.lower()
    return lower.endswith(("_ns", "_ms", "_seconds")) or "rtf" in lower


def discover_metrics(fieldnames: list[str], requested: list[str] | None = None) -> list[str]:
    baseline = {name.removeprefix("baseline_") for name in fieldnames if name.startswith("baseline_")}
    candidate = {name.removeprefix("candidate_") for name in fieldnames if name.startswith("candidate_")}
    common = sorted(stem for stem in baseline & candidate if _timing_metric(stem))
    if requested:
        normalized = [
            name.removeprefix("baseline_").removeprefix("candidate_") for name in requested
        ]
        missing = [name for name in normalized if name not in baseline & candidate]
        if missing:
            raise AnalysisError("requested timing metric pair is missing: " + ", ".join(missing))
        invalid = [name for name in normalized if not _timing_metric(name)]
        if invalid:
            raise AnalysisError("requested field is not a timing metric: " + ", ".join(invalid))
        return list(dict.fromkeys(normalized))
    if not common:
        raise AnalysisError("no paired timing fields found (expected baseline_* and candidate_* timing columns)")
    return common


def _validity_problem(row: dict[str, str]) -> str | None:
    for column in VALIDITY_COLUMNS:
        value = row.get(column, "").strip().lower()
        if value and value not in VALID_MARKERS:
            return f"{column}={row[column]!r}"
    return None


def _shape_problem(
    row: dict[str, str],
    case: str,
    known_shapes: dict[tuple[str, str], str],
) -> str | None:
    for column in SHAPE_COLUMNS:
        value = row.get(column, "").strip()
        if value:
            key = (case, column)
            previous = known_shapes.get(key)
            if previous is None:
                known_shapes[key] = value
            elif previous != value:
                return f"{column} changed within case {case!r}"
    # Some paired exports carry output descriptors on each side. If so, both
    # sides must describe the same input/output shape before their times pair,
    # and each exported descriptor must stay fixed across repetitions.
    for suffix in SHAPE_COLUMNS:
        baseline_value = row.get(f"baseline_summary_{suffix}", "").strip()
        candidate_value = row.get(f"candidate_summary_{suffix}", "").strip()
        if baseline_value and candidate_value and baseline_value != candidate_value:
            return f"baseline/candidate workload shape differs at {suffix}"
        for side, value in (("baseline", baseline_value), ("candidate", candidate_value)):
            if not value:
                continue
            column = f"{side}_summary_{suffix}"
            key = (case, column)
            previous = known_shapes.get(key)
            if previous is None:
                known_shapes[key] = value
            elif previous != value:
                return f"{column} changed within case {case!r}"
    return None


def _parse_positive(value: str) -> float | None:
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    if not math.isfinite(parsed) or parsed <= 0:
        return None
    return parsed


def _stats(values: list[float]) -> dict[str, float | int | list[float]]:
    median = statistics.median(values)
    deviations = [abs(value - median) for value in values]
    mad = statistics.median(deviations)
    return {
        "n": len(values),
        "median": median,
        "range": [min(values), max(values)],
        "mad": mad,
    }


def analyze_rows(
    rows: list[dict[str, str]],
    fieldnames: list[str],
    *,
    requested_metrics: list[str] | None = None,
) -> tuple[list[str], list[dict[str, object]]]:
    metrics = discover_metrics(fieldnames, requested_metrics)
    if not fieldnames or "case" not in fieldnames:
        raise AnalysisError("measurements.csv must contain a case column")
    groups: dict[tuple[str, str], dict[str, object]] = {}
    known_shapes: dict[tuple[str, str], str] = {}
    known_binary_hashes: dict[str, str] = {}
    case_modes: dict[str, str] = {}
    seen_pairs: set[tuple[str, str]] = set()
    invalid_reasons: list[str] = []

    for row_number, row in enumerate(rows, start=2):
        case = row.get("case", "").strip()
        if not case:
            invalid_reasons.append(f"row {row_number}: missing case")
            continue
        repetition = row.get("repetition", "").strip() or row.get("pair", "").strip()
        if not repetition:
            invalid_reasons.append(f"row {row_number} ({case}): missing repetition/pair")
            continue
        pair_key = (case, repetition)
        case_modes.setdefault(case, row.get("mode", "").strip())
        if pair_key in seen_pairs:
            invalid_reasons.append(f"row {row_number} ({case} pair {repetition}): duplicate pair")
            continue
        seen_pairs.add(pair_key)
        reason = _validity_problem(row) or _shape_problem(row, case, known_shapes)
        if reason is None:
            for column in BINARY_HASH_COLUMNS:
                value = row.get(column, "").strip()
                if value:
                    previous = known_binary_hashes.get(column)
                    if previous is not None and previous != value:
                        reason = f"{column} changed across measurement rows"
                        break
                    known_binary_hashes[column] = value
        if reason:
            invalid_reasons.append(f"row {row_number} ({case} pair {repetition}): {reason}")
            continue

        for metric in metrics:
            baseline_value = _parse_positive(row.get(f"baseline_{metric}", ""))
            candidate_value = _parse_positive(row.get(f"candidate_{metric}", ""))
            if baseline_value is None or candidate_value is None:
                invalid_reasons.append(
                    f"row {row_number} ({case} pair {repetition}, {metric}): "
                    "missing, non-finite, or non-positive paired timing"
                )
                continue
            key = (case, metric)
            group = groups.setdefault(
                key,
                {
                    "case": case,
                    "metric": metric,
                    "baseline": [],
                    "candidate": [],
                    "paired_relative_delta_percent": [],
                    "pair_ids": [],
                },
            )
            group["baseline"].append(baseline_value)
            group["candidate"].append(candidate_value)
            group["paired_relative_delta_percent"].append(
                ((candidate_value - baseline_value) / baseline_value) * 100.0
            )
            group["pair_ids"].append(repetition)

    if not groups:
        reason = invalid_reasons[0] if invalid_reasons else "no valid timing pairs"
        raise AnalysisError(f"no valid paired timings found; first excluded input: {reason}")

    reports: list[dict[str, object]] = []
    for key in sorted(groups):
        group = groups[key]
        baseline_values = group["baseline"]
        candidate_values = group["candidate"]
        deltas = group["paired_relative_delta_percent"]
        probe = "core" if case_modes.get(group["case"]) == "ordinary-eac3-core" else "api"
        baseline_hash = known_binary_hashes.get(f"baseline_{probe}_probe_binary_sha256")
        candidate_hash = known_binary_hashes.get(f"candidate_{probe}_probe_binary_sha256")
        binary_relationship = (
            "identical SHA-256"
            if baseline_hash and candidate_hash and baseline_hash == candidate_hash
            else "different SHA-256"
            if baseline_hash and candidate_hash
            else "not recorded in CSV"
        )
        reports.append(
            {
                "case": group["case"],
                "metric": group["metric"],
                "valid_pairs": len(group["pair_ids"]),
                "excluded_inputs": len(invalid_reasons),
                "probe_binary_relationship": binary_relationship,
                "baseline_self_repeat": _stats(baseline_values),
                "candidate_self_repeat": _stats(candidate_values),
                "paired_relative_delta_percent": _stats(deltas),
            }
        )
    return invalid_reasons, reports


def _number(value: float) -> str:
    return f"{value:.6g}"


def render_report(
    input_path: Path,
    excluded: list[str],
    reports: list[dict[str, object]],
    *,
    same_product_control: bool,
) -> str:
    role = (
        "caller labels this as a same-product control; binary identity is not established by this flag"
        if same_product_control
        else "paired repeatability summary (product relationship not asserted)"
    )
    lines = [
        f"Input: {input_path}",
        f"Role: {role}",
        "Paired deltas are descriptive candidate-side minus baseline-side changes; they do not imply speedup.",
        f"Valid timing summaries: {len(reports)}; excluded inputs: {len(excluded)}",
    ]
    for report in reports:
        baseline = report["baseline_self_repeat"]
        candidate = report["candidate_self_repeat"]
        delta = report["paired_relative_delta_percent"]
        lines.extend(
            [
                f"\n{report['case']} / {report['metric']} ({report['valid_pairs']} valid pairs)",
                f"  probe binary relationship: {report['probe_binary_relationship']}",
                "  baseline-side self-repeat: "
                f"median={_number(baseline['median'])}; "
                f"range=[{_number(baseline['range'][0])}, {_number(baseline['range'][1])}]; "
                f"MAD={_number(baseline['mad'])}",
                "  candidate-side self-repeat: "
                f"median={_number(candidate['median'])}; "
                f"range=[{_number(candidate['range'][0])}, {_number(candidate['range'][1])}]; "
                f"MAD={_number(candidate['mad'])}",
                "  paired relative delta (% of baseline-side value): "
                f"median={_number(delta['median'])}; "
                f"range=[{_number(delta['range'][0])}, {_number(delta['range'][1])}]; "
                f"MAD={_number(delta['mad'])}",
            ]
        )
    if excluded:
        lines.append("\nExcluded input examples:")
        lines.extend(f"  - {reason}" for reason in excluded[:10])
        if len(excluded) > 10:
            lines.append(f"  - … and {len(excluded) - 10} more")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("measurements", type=Path, help="paired measurements.csv from verify_pcm_bitexact.py")
    parser.add_argument(
        "--metric",
        action="append",
        help="timing field suffix to summarize, without baseline_/candidate_ (repeatable; default: all paired timing fields)",
    )
    parser.add_argument(
        "--same-product-control",
        action="store_true",
        help="label this input as an intentionally same-product repeatability control",
    )
    args = parser.parse_args(argv)
    try:
        with args.measurements.open(newline="", encoding="utf-8") as source:
            reader = csv.DictReader(source)
            if reader.fieldnames is None:
                raise AnalysisError("input CSV has no header")
            rows = list(reader)
        excluded, reports = analyze_rows(rows, reader.fieldnames, requested_metrics=args.metric)
        print(
            render_report(
                args.measurements,
                excluded,
                reports,
                same_product_control=args.same_product_control,
            )
        )
    except (AnalysisError, OSError, csv.Error) as error:
        print(f"PERFORMANCE REPRO ANALYSIS FAILED: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
