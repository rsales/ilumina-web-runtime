# iLumina in the browser — Boxedwine experiment

**What it's for:** an experiment to find out whether the original iLumina
Gold program can run **inside the browser**, with no Linux server and no native
Wine. It uses **Boxedwine**: an x86 CPU emulator, a Linux kernel emulator and
Wine 11, compiled to WebAssembly. Nothing is proven yet. See
[ADR 0007](../docs/decisions/0007-boxedwine-web.md) and the
[test matrix](../docs/boxedwine-test-matrix.md).

> Status: **scaffold, not yet tested end to end.** See "Test plan" below.

## Pieces

| File | Role |
|---|---|
| `probes/hello32gui.exe` + `make-probe-zip.sh` | L1: minimal 32-bit GUI program (our own code) → `probes.zip` |
| `scripts/make-overlay.sh` | discs → `ilumina.zip` (program + media laid out as a full install, plus `run.bat` / `run-noqt.bat`) |
| `Dockerfile.wasm` | builds Boxedwine with Emscripten (pinned commit) and serves it with COOP/COEP |
| `index.html` | launcher: opens `boxedwine.html` with the right parameters |
| `Dockerfile.native` + `scripts/native-entrypoint.sh` | the same emulator natively + Xpra (test bench; runs as arm64 on a Mac) |
| `compose.yaml` | `native` and `wasm` services |

## Data you provide

```
boxedwine-data/
  TinyCore15Wine11.0.zip   # Boxedwine's Wine 11 file system, ~206 MB:
                           # https://boxedwine.org/v2/23/TinyCore15Wine11.0.zip
  ilumina.zip              # made by: web/scripts/make-overlay.sh ../discs boxedwine-data
```

`make-overlay.sh` needs `zip` (and `xorriso` for `--isos`). On macOS:
`brew install zip xorriso`, or run it in the native image:
`docker compose -f web/compose.yaml run --rm native /opt/ilumina-web/make-overlay.sh /media/ilumina /state/out`.

## Run

```sh
export ILUMINA_MEDIA_DIR=$PWD/../discs BOXEDWINE_DATA_DIR=$PWD/../boxedwine-data
docker compose -f web/compose.yaml up --build native   # → http://localhost:14501 (Xpra)
docker compose -f web/compose.yaml up --build wasm     # → http://localhost:8080  (all in the browser)
```

The first launch opens the **QuickTime 6.3 installer**. Click Next/Agree to
the end, as in 2005. After that, `run.bat` starts iLumina.

## Test plan: progressive, L1 → L7

`http://localhost:8080` opens a page with one link per layer (emulator, Wine,
Director, content, Xtras, QuickTime, performance). Go in order and record
each result in [docs/boxedwine-test-matrix.md](../docs/boxedwine-test-matrix.md).
For L1, also put the probe in the data folder:
`web/probes/make-probe-zip.sh ../boxedwine-data`.
