# iLumina Web Runtime

**A web-based runtime for preserving and accessing legacy educational software.**

iLumina Web Runtime runs the **original** iLumina Gold 2.5 (2005, Portuguese
edition) in an isolated container and streams it to a web browser — video,
QuickTime VR and sound included — without installing anything on the
user's machine and without exposing a Windows desktop.

It follows the model of [nfsu2-web](https://github.com/th0mk/nfsu2-web):
**this repository contains none of iLumina's code or content.** You supply
your own discs; the runtime installs them into a private, persistent
environment on first start.

> Status: **working proof of concept.** Bible module, Valentina databases,
> QuickTime animations, QTVR tours and audio work in the browser. See
> [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## How it works

```
 browser ──HTML5/WebSocket──► Xpra ──► Wine virtual desktop (1024×768)
   ▲                           │          └─ iLumina.exe  (Director MX projector)
   └──── Opus audio ◄── PulseAudio            ├─ Xtras: Valentina, QuickTime 6.3, BuddyAPI, …
                                              └─ C:\Program Files\iLuminaPO  (installed from your CDs)
```

- **Not an emulator, not a rewrite.** iLumina is a Macromedia Director MX
  projector. Its logic is Lingo bytecode inside `.dxr`/`.cxt` files, and it
  relies on native Xtras (Valentina DB, QuickTime/QTVR, …). Static
  recompilation in the style of nfsu2-web would only give you the Director
  interpreter, without those Xtras. Wine runs the whole original stack as is.
- **Faithful install.** `runtime/install-ilumina.sh` reproduces what the
  original installer does: we ran it under Wine and diffed the result. The
  bundled QuickTime 6.3 installer runs unattended.
- **Everything from the Ubuntu archive.** Wine 9.0 and Xpra 3.1. No
  third-party repositories.

## Requirements

- Docker with Compose, on an **x86_64** host. On Apple Silicon it runs under
  emulation and is slow.
- The four iLumina Gold CDs, extracted into one folder:

  ```
  discs/
    cd1/   1.txt  Program/  iLuminaInstaller.exe …
    cd2/   2.txt  Files/Media/…
    cd3/   3.txt  Files/Media/…
    cd4/   4.txt  Files/Media/…
  ```

  `tools/extract-discs.sh` builds this from `.iso`/`.nrg` images with 7-Zip.
  `tools/verify-discs.sh discs/` checks the files against the SHA-256
  manifest in `manifests/`.

## Running

```sh
ILUMINA_MEDIA_DIR=$PWD/discs docker compose up --build
# first start: installs ~1.9 GB into the "ilumina-state" volume (2–5 min)
open http://localhost:14500/
```

### Activation code (optional)

If you own an activation code, put it in `.env` (copy `.env.example`,
which is git-ignored) and the runtime types it into the activation dialog
as soon as a browser connects:

```sh
cp .env.example .env   # fill ILUMINA_ACTIVATION_CODE (and the ILUMINA_REG_* fields)
docker compose up -d
```

iLumina issues activation codes per installation: the code is calculated
from the "#1" code shown under *Ativação por telefone*, plus the e-mail
you registered with. A code obtained for your original PC is therefore
only accepted if this installation shows the same "#1". If the code is
rejected, the dialog stays open and `/state/logs/auto-activate.log` says
so. The value is never written to the logs.

Close iLumina and the container restarts it. To reinstall from scratch,
run `docker compose down -v`.

Useful commands:

```sh
docker compose run --rm ilumina install   # only install
docker compose run --rm ilumina shell     # shell with WINEPREFIX set
```

Logs and installer screenshots are in `/state/logs` inside the volume.

## Repository layout

| Path | Purpose |
|---|---|
| `Dockerfile`, `compose.yaml` | Runtime image and local deployment |
| `runtime/entrypoint.sh` | `run` / `install` / `shell` modes, starts Xpra |
| `runtime/install-ilumina.sh` | Unattended equivalent of the original full install |
| `runtime/install-quicktime.sh` | Drives the QuickTime 6.3 installer from CD 1 |
| `runtime/run-ilumina.sh` | Starts the projector in a Wine virtual desktop |
| `tools/` | Disc extraction and verification |
| `manifests/` | SHA-256 hashes of the known disc set (hashes only) |
| `docs/` | Compatibility findings, architecture notes |

## Project goals

- Preserve the original iLumina Gold application and its educational content.
- Investigate compatibility with modern runtime environments.
- Explore containerization and application isolation.
- Provide direct access to the original application through a web browser.
- Develop a reproducible environment for running legacy educational software.
- Establish a foundation for experimenting with other legacy applications in the future.

The project does not aim to rebuild the application, reimplement its
functionality, or give users a full Windows desktop.

## Roadmap

1. ✅ Runtime compatibility investigation (Wine + Director MX + Xtras + QuickTime)
2. ✅ Reproducible containerised install and browser streaming with audio
3. ⏳ Test the remaining modules (Encyclopedia, Time Travel, Extras, PDFs)
4. ⏳ Fix the intermittent menu script error
5. ⏳ Custom landing page around the Xpra client (scaling, full screen, no toolbar)
6. ⏳ One isolated session per visitor (container per session, idle reaping)
7. ⏳ Newer Xpra (H.264/VP8) for smoother video

## Legal

iLumina is © iLúmina Criação, Comercialização e Distribuição de Softwares
Ltda / Visual Book Productions. QuickTime is © Apple. This project is not
affiliated with or endorsed by them, and it distributes none of their
software or content. It also does not bypass the product's activation
(see [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md#activation)). Use it only
with discs you own, and talk to the rights holders before you host it
publicly.

The code in this repository is released under the MIT License.
