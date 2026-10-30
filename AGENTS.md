# AGENTS.md

Instructions for AI coding agents (Claude Code, Codex, Cursor…) and human
contributors. Read this first, then [docs/README.md](docs/README.md).

## Objective

Keep **iLumina Gold 2.5**, a lost-media educational CD set from 2005,
usable from a browser by running the **original, unmodified** Windows
program (a Director MX projector plus Xtras plus QuickTime 6.3) under Wine
in a container, from discs the user owns.

Each change should serve one of the use cases in
[docs/use-cases.md](docs/use-cases.md). If it doesn't, say so before doing it.

## Method

1. **Load context**, in this order: `docs/memory/handoff.md`,
   `docs/use-cases.md`, `docs/compatibility.md`, and the ADRs in
   `docs/decisions/` that relate to your change.
2. **Plan against the use case.** State which UC you are serving and how you
   will prove it works.
3. **Change the smallest surface.**
   - Runtime behaviour → `runtime/*.sh` (shared helpers in `runtime/lib.sh`)
   - Image contents → `Dockerfile` (Ubuntu archive packages only, see ADR 0002)
   - Disc tooling → `tools/`
   - Knowledge → `docs/`
4. **Run the validation** below. Paste the evidence (log lines, `OK:` output)
   into your summary.
5. **Leave the repo better documented than you found it:**
   - new verified fact → `docs/compatibility.md` (✅/⚠️/⏳ with how you verified it)
   - gotcha you hit → `docs/memory/learnings.md`
   - architectural choice → new ADR in `docs/decisions/`
   - user-visible change → `CHANGELOG.md` under `[Unreleased]`
   - end of session → overwrite `docs/memory/handoff.md`

## Negative constraints (do NOT)

- **Do not commit or embed iLumina or QuickTime content.** That includes
  `.dxr/.cxt/.vdb` files, media, extracted text, decompiled Lingo,
  installers and **screenshots of the app**. Hashes and file names are fine.
  (ADR 0003)
- **Do not bypass, patch, reverse-engineer or generate activation, and do
  not reset the trial** (for example with throwaway prefixes). Requests in
  that direction are out of scope. (ADR 0005)
- **Do not modify the original files** in `/media/ilumina` (mounted
  read-only) or the installed program. The goal is preservation, not a remake.
- Do not add third-party apt repositories or `curl | sh` installers without a
  new ADR.
- Do not log secrets. Values from `.env` (activation code, e-mail) must
  never reach `/state/logs`, screenshots or commit messages.
- Do not write "works" for something you did not run. Mark it ⏳.
- Do not leave transient notes in canonical docs. They go in `docs/memory/`.
- **Rust port (`port/`, ADR 0008):** never commit anything derived from the
  discs (inventories, dumps, decompiled Lingo, converted media). It goes in
  `work/` or `ilumina-data/`. Never special-case the registration, trial or
  activation handlers: the engine runs the original Lingo as-is.
- Respect the port's time-boxes. If a milestone overruns while the code grows
  without closing the gap, stop and record it in ADR 0008 (stop rule).

## Validation

Pick what your change touches. All of them for changes to `Dockerfile` or
`runtime/`.

```sh
# 1. Shell syntax
for f in runtime/*.sh tools/*.sh; do bash -n "$f"; done

# 2. Discs (requires the user's discs)
tools/verify-discs.sh ./discs                      # → OK: 3297 files match …

# 3. Clean install on a fresh volume
docker compose down -v && docker compose run --rm ilumina install
#   → "QuickTime 6.3 installed" and "installation complete"

# 4. Streaming
docker compose up -d && docker compose logs ilumina | grep "xpra is ready"
#   then open http://localhost:14500 and check:
#   Home visible · Bible → Gênesis text · Multimídia → Animações plays with sound
```

```sh
# 5. Rust port (port/)
cd port && cargo test && cargo clippy
ILUMINA_DISCS=../discs cargo test        # → 29 movies, 2018 members, 446 scripts
```

GUI automation broke? Use [docs/runbooks/debugging-gui-automation.md](docs/runbooks/debugging-gui-automation.md).

## Repository map

| Path | What it is |
|---|---|
| `Dockerfile`, `compose.yaml` | Runtime image (Wine 9, Xpra 3.1, PulseAudio) and local deployment |
| `runtime/entrypoint.sh` | Modes `run` / `install` / `shell` |
| `runtime/install-ilumina.sh` | Headless equivalent of the original full install |
| `runtime/install-quicktime.sh` | Drives the bundled QuickTime 6.3 wizard |
| `runtime/run-ilumina.sh` | Starts the projector in a 1024×768 Wine desktop |
| `runtime/auto-activate.sh` | Optional: types the owner's `.env` data into first-run dialogs |
| `tools/` | Disc extraction and SHA-256 verification |
| `manifests/` | Hashes of the supported disc set |
| `port/` | Rust Director MX player (experiment, ADR 0008): `crates/rifx` parser, `crates/xtask` local tooling |
| `docs/` | Canonical documentation, see [docs/README.md](docs/README.md) |
