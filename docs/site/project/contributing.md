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

## Translations

English is the canonical technical source. Add a maintained Simplified Chinese
translation beside a page as `page.zh.md` and keep commands, flags, identifiers,
type names, and filenames unchanged. The first Chinese release intentionally
covers the high-value user paths; untranslated pages fall back to English under
`/zh/` and should not block the build. Preserve all scope qualifiers and
limitations when translating. The short translation notice at the top of
Chinese pages is intentional: English wins if the two versions differ.
