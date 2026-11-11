# sheprd.org

The page Netlify publishes at <https://sheprd.org>. It is plain HTML, CSS and
JavaScript; the docs are generated from Markdown in GitHub Actions. The root
`site/` is separate and stays with herdrui.dev.

```sh
python3 -m http.server -d website/site 8000
```

Python's server does not emulate Netlify headers or redirects. The Netlify
project uses base directory `website`, so it reads `website/netlify.toml`, which
publishes `website/site`. Only that folder is deployed, so this README and the
config stay private.

Automatic Netlify builds are disabled in the project's settings. The
[`Deploy website` workflow](../.github/workflows/site.yml) deploys on pushes to
`main` that change the published site, its docs sources, or deployment inputs.
Changes to the separate `herdrui/` worker do not trigger it. In GitHub's
**Settings → Environments → website**, restrict deployment branches to `main`
and configure environment secrets `NETLIFY_AUTH_TOKEN` (a Netlify personal access
token) and `NETLIFY_SITE_ID` (the sheprd.org project ID). The deploy job uses this
environment. CI generates the docs, then uploads the static
folder from the repository root with:

```sh
netlify deploy --prod --dir=website/site --no-build
```

This skips Netlify's build pipeline; production uploads are serialized.

`herdrui/` is the separate Cloudflare Worker for herdrui.dev, and is not
published by Netlify; see [`herdrui/README.md`](herdrui/README.md).

`site/install.sh`, `site/install.ps1` and `site/llms.txt` here must match the
copies in the root `site/`,
and `site/tests/test_website.py` checks that. Keep `site/assets/features.js` in step
with `design/sheprd-website/FEATURE-INVENTORY.md`, and describe availability
honestly: status badges and platform caveats are part of the page.

To update the social card, replace `site/assets/sheprd-social-preview.png` and
raise the `?v=` number on `og:image` and `twitter:image` in `site/index.html`.
Crawlers then treat it as a new image rather than using their cached copy.

Tests: `python3 -m unittest discover -s site/tests`.

## Docs

<https://sheprd.org/docs/> is built from Markdown in CI before the upload:
`website/docs/*.md` holds the user guides, and `docs/automations.md`,
`docs/mods.md` and `docs/cli.md` are rendered from where they are, so each
guide has one copy. `tools/docs.py` lists every page in its `NAV`, in reading
order, and writes `site/docs/`, which is not tracked: parallel pull requests
edit `docs/cli.md` all the time, and committed HTML would make each conflict.
The look is `tools/docs-theme/`.

A page in `website/docs/` starts with front matter, then its `# Title` and a
lead paragraph; `##` headings make its table of contents:

```markdown
---
title: SSH hosts
blurb: Your build box in the same window.
status: shipped
platforms: macOS, Linux, Windows
---

# SSH hosts

What this page covers, in a sentence or two.
```

`blurb` shows in navigation and search. `status` is `shipped`, `optin`,
`experimental` or `testflight`; leave it and `platforms` out of pages that
are not about one feature. A blockquote that starts with bold text, such as
`> **Note.** ...`, becomes a callout. Link pages by file name
(`[Cloud](cloud.md#sandbox-images)`) and anything else in the repository by
its relative path; links outside the pages become GitHub links. A broken
link, missing heading, or page missing from `NAV` fails the build.

Build and preview, from the repository root:

```sh
python3 -m venv /tmp/docs-venv
/tmp/docs-venv/bin/pip install --require-hashes -r website/requirements.txt
/tmp/docs-venv/bin/python website/tools/docs.py
python3 -m http.server -d website/site 8000   # then open /docs/
```

`docs.py --check` builds nothing and only reports problems; CI runs it and
`tools/test_docs.py`.
