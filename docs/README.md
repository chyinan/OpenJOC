# OpenJOC documentation repository

The published user documentation lives under [`docs/site/`](site/). Its navigation is defined in [`mkdocs.yml`](../mkdocs.yml).

## Canonical site owners

- [Introduction and capabilities](site/getting-started/introduction.md) — user-facing overview and entry points.
- [Speaker rendering](site/using/speaker-rendering.md) — renderer, layout, level, and timing behavior.
- [Reconstructed ADM export](site/using/reconstructed-adm-export.md) — current export contract and report semantics.
- [Decoded Objects vs authored Objects](site/concepts/decoded-vs-authored-objects.md) — identity and recovery boundary.
- [Known limitations](site/compatibility/known-limitations.md) — current user-visible non-claims.
- [Clean-room methodology](site/project/clean-room-methodology.md) — permitted sources and evidence classes.

## Repository-only material

The following directories remain outside the published site by design:

- [`archive/`](archive/) — retained historical contracts and older release material;
- [`research/`](research/) — dated experiments, negative results, and implementation history;
- [`release/`](release/) — packaging, corresponding-source, and distribution evidence;
- [`design-plans/`](design-plans/) and [`implementation-plans/`](implementation-plans/) — engineering planning records;
- [`integration/evidence/`](integration/evidence/) — local and release-gate evidence, including machine-specific capture records.

These records are not silently rewritten to match current user documentation. A current fact belongs to the site owner above; a historical result stays with its dated record.

## Maintenance

Install the pinned docs dependencies from the repository root and run:

```sh
python -m pip install -r requirements-docs.txt
python -m mkdocs serve
python -m mkdocs build --strict
```

The [contributor page](site/project/contributing.md) explains the smallest workflow for adding a page or changing navigation.
