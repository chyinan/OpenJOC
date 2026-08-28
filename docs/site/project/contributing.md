# Contributing

Start with the repository's [contribution guide](https://github.com/chyinan/OpenJOC/blob/master/CONTRIBUTING.md) for code, clean-room, testing, and repository hygiene rules.

## Documentation workflow

The site uses MkDocs with Material for MkDocs. Markdown pages live under `docs/site/`; `mkdocs.yml` owns navigation; `docs/site/assets/stylesheets/extra.css` contains the small visual layer; `.github/workflows/docs.yml` builds and deploys the site.

From the repository root:

```sh
py -3 -m venv .venv-docs
.venv-docs\\Scripts\\python -m pip install -r requirements-docs.txt
.venv-docs\\Scripts\\python -m mkdocs serve
.venv-docs\\Scripts\\python -m mkdocs build --strict
```

On Unix-like hosts, activate the environment and use `python -m mkdocs serve` or `python -m mkdocs build --strict`.

Add a page under the closest information-architecture section, add it to `mkdocs.yml`, and run the strict build before committing. Keep one canonical owner for each technical fact. Leave dated research, release evidence, and internal implementation plans under their existing repository directories unless they have a clear user-facing purpose.

## Research contributions

For difficult codec, renderer, storage, or hardware-validation work, read
[Open Problems & Contribution Opportunities](open-problems.md) first. Start a
Discussion before a large research implementation so existing evidence
boundaries and stop conditions are visible. A useful contribution needs a
reproducible test, a clear evidence boundary, and clean-room-compliant
provenance. Small deterministic fixes and ordinary documentation changes can
go straight through the normal contribution workflow.

## Translations

English is the canonical technical source. Every current site page has a
maintained Simplified Chinese translation beside it as `page.zh.md`. Keep
commands, flags, identifiers, type names, and filenames unchanged. Preserve all
scope qualifiers and limitations when translating. The short translation notice
at the top of Chinese pages is intentional: English wins if the two versions
differ. New pages should add both language files together.
