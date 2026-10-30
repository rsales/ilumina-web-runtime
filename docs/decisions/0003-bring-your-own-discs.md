# 0003 — Ship no content; the user supplies the discs

- Status: Accepted; amended by 0008 (derived local data: `work/`, `ilumina-data/`, never committed)
- Date: 2026-10-28

## Problem
iLumina, its media and the bundled QuickTime installer are copyrighted.
The project must be publishable and must not redistribute them.

## Decision
The image and the repository contain only our code. The four discs are
bind-mounted read-only at `/media/ilumina/cd1..cd4`. The install happens
on the user's machine into the `/state` volume. A SHA-256 manifest
(hashes and paths only) identifies the supported release.

## Alternatives considered
- **Bake the install into the image**: easiest to run, but the image
  could never be published.

## Consequences
- ✅ Same model as nfsu2-web. The repo can be public.
- ❌ First start takes a few minutes, and users must extract their discs.
- Never commit `.dxr/.cxt/.vdb`, media, screenshots of the app or the
  `/state` volume (enforced by `.gitignore`).
