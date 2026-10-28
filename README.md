# iLumina Web Runtime

**Use iLumina Gold again, in a browser, from the discs you already own.**

iLumina Gold (2005) is a Portuguese-language Bible and encyclopedia on four
CDs: it has 3D animations, QuickTime VR tours of Jerusalem, maps, a timeline
and several Bible translations. It is no longer sold. The installer expects
Windows XP and QuickTime 6, and the activation server is gone. On a modern
computer the discs are effectively lost media.

This project makes those discs usable again **without rewriting anything**.
It runs the original program, unchanged, in an isolated container, and you
use it from a browser tab, sound and video included.

| You want to… | This gives you |
|---|---|
| open your old iLumina on a modern Mac/Linux/Windows | one `docker compose up`, then `http://localhost:14500` |
| preserve the software as it was | the original executable, Xtras, databases and QuickTime, as is |
| check that your disc copy is complete and intact | `tools/verify-discs.sh` against a SHA-256 manifest |
| understand how the software works / run other Director titles | the findings in [`docs/`](docs/README.md) |

> **Status:** working proof of concept. The Bible, animations, QTVR tours and
> audio work. See [docs/compatibility.md](docs/compatibility.md).

## Quick start

You need Docker and the four iLumina Gold CDs (as images or copied folders).

```sh
git clone <this repo> && cd ilumina-web-runtime
tools/extract-discs.sh ./discs cd1.iso cd2.iso cd3.iso cd4.iso   # or copy the discs into discs/cd1..cd4
tools/verify-discs.sh ./discs
cp .env.example .env                                              # set ILUMINA_MEDIA_DIR=./discs
docker compose up -d --build
open http://localhost:14500
```

The first start installs the program into a Docker volume. That takes 2–5
minutes and about 2 GB.

## What to expect

- **Activation.** Unactivated copies run as a 15-day trial. If you have an
  activation code, put it in `.env`. Codes are tied to the installation,
  so a code from your old PC may be rejected. See
  [docs/use-cases.md](docs/use-cases.md) (UC4).
- **Apple Silicon: not supported.** In Docker on an M-series Mac, 32-bit x86
  code can't run at all: QEMU fails with `status c0000018`, and Rosetta for
  Linux doesn't support 32-bit compatibility mode (`rosetta error: invalid gdt
  selector index 4`). That holds for every Wine build. Use an x86_64 machine or
  VPS. Details: [docs/memory/learnings.md](docs/memory/learnings.md).
- **One user at a time** per container. It is meant for personal or
  archival use, not public hosting.
- Picking an item from the top menu bar **sometimes** shows a Lingo error.
  Click "Yes" and it carries on.

## What's in the box

This repository contains **no iLumina code or content**. It contains the
runtime (Dockerfile + scripts), the disc tools, a hash manifest and the
documentation. You supply the discs, the same model as
[nfsu2-web](https://github.com/th0mk/nfsu2-web).

- How it works and why: [docs/architecture.md](docs/architecture.md)
- Contributing or using an AI agent here: [AGENTS.md](AGENTS.md) and [docs/](docs/README.md)
- Changes: [CHANGELOG.md](CHANGELOG.md)

## Legal

iLumina is © iLúmina Criação, Comercialização e Distribuição de Softwares
Ltda / Visual Book Productions. QuickTime is © Apple. This project is not
affiliated with them and does not bypass the product's activation. Use it
only with discs you own, and talk to the rights holders before you host it
for others. The code here is MIT-licensed.
