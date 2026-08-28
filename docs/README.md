# OpenJOC documentation repository

The published user documentation lives under [`docs/site/index.md`](site/index.md). Its navigation is defined in [`mkdocs.yml`](../mkdocs.yml).

## Canonical site owners

- [Introduction and capabilities](site/getting-started/introduction.md) — user-facing overview and entry points.
- [Speaker rendering](site/using/speaker-rendering.md) — renderer, layout, level, and timing behavior.
- [Reconstructed ADM export](site/using/reconstructed-adm-export.md) — current export contract and report semantics.
- [Decoded Objects vs authored Objects](site/concepts/decoded-vs-authored-objects.md) — identity and recovery boundary.
- [Known limitations](site/compatibility/known-limitations.md) — current user-visible non-claims.
- [Clean-room methodology](site/project/clean-room-methodology.md) — permitted sources and evidence classes.
- [Open Problems & Contribution Opportunities](site/project/open-problems.md) — stable contribution directions and evidence expectations.

## Repository-only material

The following directories remain outside the published site by design:

- [`archive/`](archive/README.md) — retained historical contracts and older release material;
- [`research/`](research/README.md) — dated experiments, negative results, and implementation history;
- [`release/`](release/README.md) — packaging, corresponding-source, and distribution evidence;
- [`integration/evidence/`](integration/evidence/windows-lav-multichannel-2026-08-23.json) — local and release-gate evidence, including machine-specific capture records.

Engineering planning records are kept separately under the top-level
[`planning/`](../planning/README.md) directory and are not part of the
published documentation.

These records are not silently rewritten to match current user documentation. A current fact belongs to the site owner above; a historical result stays with its dated record.

## Maintenance

Install the pinned docs dependencies from the repository root and run:

```sh
python -m pip install -r requirements-docs.txt
python -m mkdocs serve
python -m mkdocs build --strict
```

The [contributor page](site/project/contributing.md) explains the smallest workflow for adding a page or changing navigation.

The site is English-canonical and also builds a maintained Simplified Chinese
translation layer with `mkdocs-static-i18n`. Every current site page has a
Chinese counterpart using the suffix form `page.zh.md` beside the English
owner. Future pages should add both language files together; the configured
fallback remains available as a safety net for incomplete future translations.
