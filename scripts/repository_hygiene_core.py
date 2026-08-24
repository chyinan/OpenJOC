# pattern: Functional Core
"""Pure repository-documentation hygiene validation."""

from __future__ import annotations

import json
import posixpath
import re
from collections.abc import Mapping, Set
from urllib.parse import unquote


ROOT_ARTIFACT_PATTERNS = (
    re.compile(r"^OPENJOC_.*_HANDOFF\.md$", re.IGNORECASE),
    re.compile(r"^OPENJOC_.*_BASELINE\.md$", re.IGNORECASE),
    re.compile(r"^OPENJOC_.*_REVIEW.*\.md$", re.IGNORECASE),
    re.compile(r"^OPENJOC_.*_RESULT.*\.md$", re.IGNORECASE),
    re.compile(r"^PROGRESS-.*\.md$", re.IGNORECASE),
    re.compile(r"^.*_AUDIT\.md$", re.IGNORECASE),
)

RETIRED_STALE_DOCUMENTS = (
    "docs/FUTURE_PLAYER_ADAPTERS.md",
    "docs/INTEGRATION_API_CURRENT_STATE.md",
    "docs/integration/FFMPEG_NATIVE_FUTURE.md",
)

INLINE_LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
HTML_LINK = re.compile(r"\b(?:href|src)\s*=\s*[\"']([^\"']+)[\"']", re.IGNORECASE)
HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*#*\s*$", re.MULTILINE)
FENCE = re.compile(r"^\s*(`{3,}|~{3,})")


def root_artifact_errors(tracked_paths: Set[str]) -> list[str]:
    """Return tracked development-report violations at repository root."""

    violations = []
    for path in sorted(tracked_paths):
        if "/" in path:
            continue
        if any(pattern.fullmatch(path) for pattern in ROOT_ARTIFACT_PATTERNS):
            violations.append(f"root development artifact is tracked: {path}")
    return violations


def _split_destination(raw: str) -> tuple[str, str]:
    destination = raw.strip()
    if destination.startswith("<") and ">" in destination:
        destination = destination[1 : destination.index(">")]
    elif " " in destination:
        destination = destination.split(maxsplit=1)[0]
    destination = unquote(destination)
    path, separator, fragment = destination.partition("#")
    path = path.partition("?")[0]
    return path, fragment if separator else ""


def _github_anchor(value: str) -> str:
    without_tags = re.sub(r"<[^>]+>", "", value)
    without_markup = without_tags.replace("`", "").strip().lower()
    without_punctuation = re.sub(r"[^\w\- ]", "", without_markup)
    return without_punctuation.replace(" ", "-")


def _anchors(markdown: str) -> set[str]:
    anchors: set[str] = set()
    occurrences: dict[str, int] = {}
    for match in HEADING.finditer(markdown):
        base = _github_anchor(match.group(1))
        duplicate = occurrences.get(base, 0)
        occurrences[base] = duplicate + 1
        anchors.add(base if duplicate == 0 else f"{base}-{duplicate}")
    return anchors


def _has_unclosed_fence(markdown: str) -> bool:
    opening: tuple[str, int] | None = None
    for line in markdown.splitlines():
        match = FENCE.match(line)
        if not match:
            continue
        marker = match.group(1)
        if opening is None:
            opening = (marker[0], len(marker))
        elif marker[0] == opening[0] and len(marker) >= opening[1]:
            opening = None
    return opening is not None


def markdown_link_errors(
    documents: Mapping[str, str], tracked_paths: Set[str]
) -> list[str]:
    """Validate repository-local inline Markdown links and anchors."""

    errors: list[str] = []
    normalized_tracked = {path.replace("\\", "/") for path in tracked_paths}
    for source in sorted(documents):
        source_directory = posixpath.dirname(source)
        if _has_unclosed_fence(documents[source]):
            errors.append(f"{source}: unclosed Markdown fence")
        destinations = [
            match.group(1) for match in INLINE_LINK.finditer(documents[source])
        ]
        destinations.extend(
            match.group(1) for match in HTML_LINK.finditer(documents[source])
        )
        for destination in destinations:
            path_part, fragment = _split_destination(destination)
            lowered = path_part.lower()
            if (
                lowered.startswith(("http://", "https://", "mailto:", "data:"))
                or path_part.startswith("//")
            ):
                continue

            if path_part.startswith("/"):
                target = posixpath.normpath(path_part.lstrip("/"))
            elif path_part:
                target = posixpath.normpath(posixpath.join(source_directory, path_part))
            else:
                target = source

            if target == ".." or target.startswith("../"):
                errors.append(f"{source}: local link escapes repository: {destination}")
                continue

            if path_part.endswith("/"):
                target = posixpath.join(target, "README.md")

            if target not in normalized_tracked:
                errors.append(f"{source}: missing local link target: {target}")
                continue

            if fragment and target.lower().endswith(".md"):
                target_document = documents.get(target)
                if target_document is None or fragment not in _anchors(target_document):
                    errors.append(
                        f"{source}: missing Markdown anchor #{fragment} in {target}"
                    )
    return errors


def _required_match(files: Mapping[str, str], path: str, pattern: str) -> re.Match[str] | None:
    return re.search(pattern, files.get(path, ""), re.MULTILINE)


def documentation_consistency_errors(files: Mapping[str, str]) -> list[str]:
    """Cross-check current docs against source and public package metadata."""

    errors: list[str] = []
    header = "crates/openjoc-capi/include/openjoc.h"
    major_match = _required_match(
        files, header, r"^#define OPENJOC_ABI_VERSION_MAJOR\s+(\d+)u$"
    )
    minor_match = _required_match(
        files, header, r"^#define OPENJOC_ABI_VERSION_MINOR\s+(\d+)u$"
    )
    if not major_match or not minor_match:
        errors.append("public header does not expose parseable C ABI major/minor macros")
        abi_major = abi_minor = None
    else:
        abi_major = int(major_match.group(1))
        abi_minor = int(minor_match.group(1))
        abi = f"{abi_major}.{abi_minor}"
        if f"The ABI is `{abi}-experimental`" not in files.get("docs/C_API.md", ""):
            errors.append(f"docs/C_API.md does not document current C ABI {abi}")
        if f"Versioned C ABI {abi}" not in files.get("docs/CAPABILITIES.md", ""):
            errors.append(f"docs/CAPABILITIES.md does not document current C ABI {abi}")

    cargo = files.get("Cargo.toml", "")
    version_match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    project_version = version_match.group(1) if version_match else None
    if project_version is None:
        errors.append("Cargo.toml does not expose a parseable workspace package version")
    elif f"## [{project_version}]" not in files.get("CHANGELOG.md", ""):
        errors.append(f"CHANGELOG.md has no entry for current package {project_version}")

    try:
        manifest = json.loads(files.get("packaging/player/PLAYER_PACKAGE_MANIFEST.json", ""))
    except (json.JSONDecodeError, TypeError):
        errors.append("player package manifest is not valid JSON")
    else:
        manifest_openjoc = manifest.get("openjoc", {})
        if project_version and manifest_openjoc.get("version") != project_version:
            errors.append("player package manifest version does not match Cargo.toml")
        manifest_abi = manifest_openjoc.get("c_abi", {})
        if abi_major is not None and (
            manifest_abi.get("major") != abi_major
            or manifest_abi.get("minor") != abi_minor
        ):
            errors.append("player package manifest C ABI does not match openjoc.h")

    layout_match = _required_match(
        files,
        "crates/openjoc-scene/src/speaker_layouts.rs",
        r"^pub const MAX_CUSTOM_SPEAKERS: usize = (\d+);$",
    )
    if not layout_match:
        errors.append("speaker layout source does not expose MAX_CUSTOM_SPEAKERS")
    else:
        limit = layout_match.group(1)
        for path in (
            "README.md",
            "docs/CAPABILITIES.md",
            "docs/CUSTOM_SPEAKER_LAYOUTS.md",
        ):
            if not re.search(rf"\b{re.escape(limit)}\b.*output channels", files.get(path, ""), re.DOTALL):
                errors.append(f"{path} does not document the source custom-layout limit {limit}")

    readme = files.get("README.md", "")
    if not re.search(
        r"\b(?:OpenJOC|release|version)\s+v?\d+\.\d+(?:\.\d+)?\b"
        r"|\bv\d+\.\d+(?:\.\d+)?\b",
        readme,
        re.IGNORECASE,
    ):
        pass
    else:
        errors.append("README.md pins a product release version")
    if re.search(r"\bRust\s+\d+\.\d+", readme, re.IGNORECASE):
        errors.append("README.md pins a Rust toolchain version")
    if re.search(r"\bC ABI\s+\d+\.\d+", readme, re.IGNORECASE):
        errors.append("README.md pins a C ABI version")
    if "github.com/chyinan/OpenJOC/releases/latest" not in readme:
        errors.append("README.md does not link to the latest release")
    if re.search(
        r"https://github\.com/chyinan/OpenJOC/releases/"
        r"(?!latest(?:[)#?\s]|$))",
        readme,
    ):
        errors.append("README.md links to a version-pinned release")
    for required in ("install.bat", "--layout-file", "C ABI", "64 output channels"):
        if required not in readme:
            errors.append(f"README.md is missing current entry-point claim: {required}")

    docs_index = files.get("docs/README.md", "")
    if re.search(r"What does OpenJOC\s+\d+\.\d+", docs_index):
        errors.append("docs/README.md presents an old release as current truth")

    capabilities = files.get("docs/CAPABILITIES.md", "")
    cli_synopsis = re.search(r"^openjoc render-joc FILE[^\r\n]*$", capabilities, re.MULTILINE)
    if not cli_synopsis or "--layout-file" not in cli_synopsis.group(0):
        errors.append(
            "docs/CAPABILITIES.md omits --layout-file from the canonical CLI synopsis"
        )

    current_docs = "\n".join(
        text
        for path, text in files.items()
        if path == "README.md"
        or (
            path.startswith("docs/")
            and not path.startswith(("docs/archive/", "docs/research/"))
            and path != "docs/PROVENANCE.md"
        )
    )
    stale_phrases = (
        "This document records the OpenJOC 0.8 integration boundary",
        "Player-specific acceptance and an FFmpeg wrapper remain later phases",
        "It does not implement mpv integration",
        "COM filter, media-type negotiation, allocator",
        "current 0.9 development commit",
        "The 0.9.1 `render-joc` command",
        "| Semantic | Status | 0.9 treatment |",
    )
    for phrase in stale_phrases:
        if phrase in current_docs:
            errors.append(f"current documentation retains stale claim: {phrase}")

    for path in RETIRED_STALE_DOCUMENTS:
        if path in files:
            errors.append(f"retired stale document still exists: {path}")

    directshow_layouts = ("Stereo", "5.1", "7.1", "5.1.2", "5.1.4", "7.1.2", "7.1.4")
    for path in ("docs/KNOWN_LIMITATIONS.md", "docs/integration/LAV_FILTERS_OPENJOC.md"):
        document = files.get(path, "")
        normalized_document = " ".join(document.split())
        if any(layout not in document for layout in directshow_layouts):
            errors.append(f"{path} does not preserve the fixed DirectShow layout contract")
        if "AUTO_NOT_RELIABLE" not in document:
            errors.append(f"{path} does not preserve AUTO_NOT_RELIABLE")
        if "Physical multichannel hardware is not verified" not in normalized_document:
            errors.append(f"{path} does not preserve the physical hardware boundary")

    return errors
