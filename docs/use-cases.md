# Use cases

What this project is *for*, written from the side of the person with the
problem. Each use case lists how it is served today and how well (✅ / ⚠️ / ⏳).

## Who has the problem

- **Owners of the discs.** They paid for iLumina, still have the CDs, and
  can't run them: no Windows XP, no QuickTime 6, no activation server.
- **Preservationists / archivists.** They want the software kept *as it
  was*, reproducibly, not a remake.
- **Developers studying Director-era multimedia.** They want a working
  reference to learn file formats, Xtras and behaviour from.

## UC1 — "I want to use my iLumina again" ✅

- **Pain:** the installer and the program no longer run on a current OS.
- **Solution:** `docker compose up`, then open a browser tab. The original
  program runs in Wine inside the container; Xpra streams picture and sound.
- **Done when:** the Home screen appears in the browser and the Bible,
  animations and VR tours open.
- **How:** [runbooks/run-locally.md](runbooks/run-locally.md)

## UC2 — "Install it the way the original installer would, without swapping discs" ✅

- **Pain:** the original installer asks you to insert CD 2, 3 and 4 in turn,
  then runs a separate QuickTime installer with 7+ wizard pages.
- **Solution:** `runtime/install-ilumina.sh` copies exactly the files the
  real installer copies (verified by diff). `runtime/install-quicktime.sh`
  clicks through the bundled QuickTime 6.3 installer unattended.
- **Done when:** `/state/.installed` exists and `QuickTime.qts` is in `system32`.
- **Background:** [decisions/0004](decisions/0004-reproduce-installer-headless.md)

## UC3 — "Is my disc copy complete and intact?" ✅

- **Pain:** discs from 2005 rot. Old images (`.nrg`, Toast HFS hybrids) are
  awkward to open, and you can't tell whether a copy is complete.
- **Solution:** `tools/extract-discs.sh` (⏳ not yet run end-to-end on real
  images) and `tools/verify-discs.sh`, which checks all 3,298 files against
  `manifests/ilumina-gold-2.50-ptbr.sha256`.
- **Done when:** `verify-discs.sh` prints `OK: 3298 files match`.

## UC4 — "Activate my copy" ⚠️

- **Pain:** activation needed `visual-book.com`, which is gone.
  Unactivated copies stop after a 15-day trial.
- **Solution:** if you have a valid code, set `ILUMINA_ACTIVATION_CODE` in
  `.env` and it is typed into the dialog for you. The trial clock and the
  activation persist in the `/state` volume like on a real PC.
- **Limitation:** codes are issued per installation (install code "#1" +
  e-mail "#2"). A code from another PC is normally rejected.
- **Out of scope:** generating codes or bypassing the activation, see
  [decisions/0005](decisions/0005-no-activation-bypass.md).

## UC5 — "Learn how a Director MX title works" ✅

- **Pain:** Director titles depend on undocumented formats and third-party
  Xtras that no longer exist anywhere else.
- **Solution:** [compatibility.md](compatibility.md) records the engine
  version, the Xtra inventory, the `iLuminaInfo.txt` contract, the
  installer's behaviour and what works under Wine.

## UC6 — "Share access with my family / a small group" ⏳

- **Pain:** one container serves one user at a time.
- **Planned:** one container per visitor from a pre-installed snapshot,
  see [architecture.md](architecture.md#multi-user-hosting-next-step).

## Explicitly not goals

- Rebuilding or "modernising" iLumina's interface or content.
- Giving users a full Windows desktop.
- Distributing iLumina, QuickTime or any of their files.
- Defeating copy protection or activation.
