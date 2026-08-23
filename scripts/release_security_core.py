#!/usr/bin/env python3
"""Pure release-environment filtering and non-disclosing scan rules."""

# pattern: Functional Core

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Iterable, Mapping


_SENSITIVE_NAME = re.compile(
    r"(?:^|_)(?:"
    r"API_?KEY|CLIENT_?SECRET|SECRET|TOKEN|PASSWORD|PASSWD|"
    r"CREDENTIALS?|PRIVATE_?KEY|AUTH(?:ORIZATION)?|COOKIE|SESSION"
    r")(?:_|$)",
    re.IGNORECASE,
)
_ASSIGNMENT = re.compile(
    r"^\s*(?:(?:export|set)\s+)?"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?P<value>.*)$",
    re.IGNORECASE,
)
_CREDENTIAL_PATTERNS = (
    ("github_token", re.compile(r"(?<![A-Za-z0-9])gh[pousr]_[A-Za-z0-9]{30,}")),
    ("service_token", re.compile(r"(?<![A-Za-z0-9])(?:sk-|figd_)[A-Za-z0-9_-]{20,}")),
    (
        "jwt",
        re.compile(
            r"(?<![A-Za-z0-9_-])eyJ[A-Za-z0-9_-]{8,}\."
            r"[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"
        ),
    ),
)
_WINDOWS_USER_HOME = re.compile(
    r"(?i)(?<![A-Za-z0-9])[A-Z]:\\Users\\[^\\\s\"'=]+"
)
_SOURCE_SUFFIXES = frozenset(
    {
        ".c",
        ".cc",
        ".cpp",
        ".cxx",
        ".go",
        ".h",
        ".hh",
        ".hpp",
        ".java",
        ".js",
        ".py",
        ".rb",
        ".rs",
        ".ts",
    }
)
_KNOWN_SENSITIVE_NAMES = frozenset(
    "_".join(parts)
    for parts in (
        ("FIGMA", "API", "KEY"),
        ("JD", "APP", "KEY"),
        ("PDD", "CLIENT", "SECRET"),
        ("STITCH", "API", "KEY"),
        ("TAOBAO", "APP", "KEY"),
        ("TAOBAO", "APP", "SECRET"),
    )
)


@dataclass(frozen=True)
class Finding:
    """A security finding that deliberately carries no matched value."""

    category: str
    subject: str
    line_number: int
    indicator: str


def is_sensitive_environment_name(name: str) -> bool:
    """Return whether an environment name is unsafe for a release process."""

    upper = name.upper()
    return upper in _KNOWN_SENSITIVE_NAMES or _SENSITIVE_NAME.search(upper) is not None


def build_release_environment(
    source: Mapping[str, str],
    *,
    allowed_names: Iterable[str],
    overrides: Mapping[str, str],
) -> dict[str, str]:
    """Return an explicit allowlist environment with validated overrides."""

    source_by_casefold = {name.casefold(): (name, value) for name, value in source.items()}
    result: dict[str, str] = {}
    for allowed_name in allowed_names:
        if is_sensitive_environment_name(allowed_name):
            raise ValueError(
                f"sensitive environment variable cannot be allowlisted: {allowed_name}"
            )
        item = source_by_casefold.get(allowed_name.casefold())
        if item is not None:
            result[allowed_name] = item[1]

    for name, value in overrides.items():
        if not name or "=" in name:
            raise ValueError("invalid environment variable name")
        if is_sensitive_environment_name(name):
            raise ValueError(
                f"sensitive environment variable cannot be overridden: {name}"
            )
        result[name] = value
    return dict(sorted(result.items(), key=lambda item: item[0].casefold()))


def _is_nonempty_assignment_value(value: str) -> bool:
    return bool(value.strip().strip("'\""))


def _is_source_subject(subject: str) -> bool:
    entry = subject.rsplit("!", maxsplit=1)[-1]
    normalized = entry.replace("\\", "/")
    filename = normalized.rsplit("/", maxsplit=1)[-1]
    if "." not in filename:
        return False
    return ("." + filename.rsplit(".", maxsplit=1)[-1]).casefold() in _SOURCE_SUFFIXES


def scan_text(
    text: str,
    *,
    subject: str,
    private_path_markers: Iterable[str],
) -> tuple[Finding, ...]:
    """Classify secret-like assignments and private paths without retaining values."""

    markers = tuple(marker for marker in private_path_markers if marker)
    source_subject = _is_source_subject(subject)
    findings: list[Finding] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        assignment = _ASSIGNMENT.match(line)
        assigned_name = assignment.group("name") if assignment else None
        known_assignment = (assigned_name or "").upper() in _KNOWN_SENSITIVE_NAMES
        assignment_reported = False
        if (
            assignment
            and is_sensitive_environment_name(assigned_name or "")
            and (not source_subject or known_assignment)
        ):
            if _is_nonempty_assignment_value(assignment.group("value")):
                findings.append(
                    Finding(
                        category="sensitive_assignment",
                        subject=subject,
                        line_number=line_number,
                        indicator=assigned_name or "sensitive_name",
                    )
                )
                assignment_reported = True
        if not assignment_reported:
            upper_line = line.upper()
            for known_name in sorted(_KNOWN_SENSITIVE_NAMES):
                if known_name in upper_line:
                    findings.append(
                        Finding(
                            category="known_name",
                            subject=subject,
                            line_number=line_number,
                            indicator=known_name,
                        )
                    )

        line_casefold = line.casefold()
        explicit_private_marker = False
        for index, marker in enumerate(markers, start=1):
            if marker.casefold() in line_casefold:
                explicit_private_marker = True
                findings.append(
                    Finding(
                        category="private_path",
                        subject=subject,
                        line_number=line_number,
                        indicator=f"private_path_marker_{index}",
                    )
                )
        if not explicit_private_marker:
            if _WINDOWS_USER_HOME.search(line):
                findings.append(
                    Finding(
                        category="private_path",
                        subject=subject,
                        line_number=line_number,
                        indicator="windows_user_home",
                    )
                )
        for indicator, pattern in _CREDENTIAL_PATTERNS:
            if pattern.search(line):
                findings.append(
                    Finding(
                        category="credential_pattern",
                        subject=subject,
                        line_number=line_number,
                        indicator=indicator,
                    )
                )
    return tuple(findings)


def format_finding(finding: Finding) -> str:
    """Render metadata only; never include matched text or values."""

    return (
        f"{finding.category}: {finding.subject}:{finding.line_number} "
        f"indicator={finding.indicator}"
    )
