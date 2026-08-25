# portal

The Calumma landing page — a Bun + React + TypeScript single-page site that hands out the
installers, deployed to GitHub Pages.

```
bun install
bun run dev      # local, on :5173
bun run build    # type-check + bundle into dist/
```

## What it does

One file of application code, `src/App.tsx`, with two pages behind a hash route:

- **Download** (`#/`) — one card per platform, pointed at the newest release.
- **Releases** (`#/releases`) — every release, newest first, with its date, a prerelease
  badge where it applies, an installer button per asset, and a link to its notes on GitHub.

Hash routing rather than paths, because Pages serves static files: `/releases` would 404 on a
hard refresh unless the site shipped a catch-all, while a hash never leaves the one document.

Both pages come from **one** request for the release list, made on load — the API is
unauthenticated and allows 60 requests per hour per IP, so a visitor who reads both pages
should still only spend one. Each platform is matched against a release's assets: `.dmg` for
macOS, `.msi`/`.exe` for Windows, `.AppImage`/`.deb`/`.rpm` for Linux. A platform with a
matching asset gets a download button carrying the file size and a link to its `.sha256`; a
platform without one says **Coming soon**.

Nothing hardcodes which platforms have shipped. Today only the macOS `.dmg` exists, so only
that card is a download — the day a Windows or Linux installer lands in a release, that card
becomes one on its own with no change here.

Because the fetch happens in the browser rather than at build time, **a new release shows up
on the site the moment it is published** — there is no redeploy to wait for, and the workflow
below deliberately has no `release` trigger.

If the API cannot be reached the cards fall back to "Releases unavailable" and the footer
points at the releases page.

## Deployment

`.github/workflows/portal.yml` builds and publishes to Pages, and runs **only** when the
push to `main` touches `portal/**` or the workflow file itself — an engine-only commit never
starts it. The site serves from `https://janezsedeljsak.github.io/calumma-v2/`, which
is why `vite.config.ts` sets `base: "/calumma-v2/"` — change that if the repo is renamed.

**One-time setup:** in the repository's Settings → Pages, set *Source* to **GitHub Actions**.
The workflow cannot do this for you and the first deploy fails without it.

`main.yml` ignores `portal/**` so a landing-page change does not spend a macOS runner on the
engine test suite.

## Styling

Follows `docs/STYLE.md`: island surfaces at `radius.island` with the thin `islandBorder`
edge, controls with a `controlBorder` edge, hover shifting luminance rather than border
weight, the brand wordmark in the teal→orange gradient, custom inline SVG rather than an
icon pack, and light/dark from `prefers-color-scheme`.

Two deliberate departures, both because this is a web page and not the dense desktop shell:

- `src/styles.css` transcribes the values from `design/tokens.json` by hand. The Swift token
  pipeline (`./manage.py tokens`) has no web target, and wiring one up for a page this size
  would be more machinery than it saves. The list is short — keep the two in sync manually.
- Buttons are 36px rather than `control.height` (28px). A 28px call-to-action on a landing
  page is a hit target sized for a packed tool panel, not for a page whose whole job is one
  click.
