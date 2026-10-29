# Compatibility findings — iLumina Gold 2.5 (Windows build)

Status as of 2026-10-28. Environment: Ubuntu 24.04, Wine 9.0 (32-bit prefix),
QuickTime 6.3 from CD 1, Xpra 3.1.5 HTML5 client, Chromium.

## What the software is

| Item | Finding |
|---|---|
| Engine | Macromedia **Director MX (9.0)** projector — `DRCF` version `0x73A`; movies are protected `MV93` (`.dxr` / `.cxt`, no Lingo source) |
| Discs | 4 hybrid Mac/PC CDs. CD 1 = program + databases + part of the media; CDs 2–4 = `Files/Media` only. Each disc root has a marker `N.txt` ("This is a media ID file") |
| Databases | **Valentina for Director** (`V4MD.x32`): `*.vdb/.dat/.blb/.ind` (Bible translations KJV, NTLH, RA, RC; encyclopedia; media index …) |
| Video / VR | **QuickTime 6.3** (`QT3Asset.x32`, "QuickTime6 Asset"): `.mov` animations and videos, **QTVR** panoramas and objects |
| Other Xtras | BuddyAPI, FileIO, BinaryIO, vList, TreeView, OSControl, Mui Dialog, Impressario (PDF), PrintOMatic, NetLingo/INetURL, DirectImage, DirectEmail, Flash Asset, Shockwave 3D |
| Install info | `iLuminaInfo.txt` — a Lingo property list read by `PathMgr` in `GCode.cxt` |

## Installer behaviour (verified by diffing a real install)

`iLuminaInstaller.exe` runs fine under Wine, including the four disc-swap
prompts (simulated by re-pointing the `D:` symlink). A full install:

1. copies `CD1:\Program\*` except Mac-only files → `C:\Program Files\iLuminaPO`
2. copies `CD2..4:\Files\Media\*` into `Files\Media` (≈1.9 GB total)
3. writes `iLuminaInfo.txt`:
   `[#applicationPath: "C:\Program Files\iLuminaPO\", #Full: -1, #DVDOrCD: #cd, #DvDDrive: "", #CDDrive: "D:\", #edition: "Gold", #version: 2.50, #installRanFromApp: 0]`
4. adds an Uninstall registry key (the Start-menu shortcut step fails under Wine, harmless)
5. launches the QuickTime 6.3 installer from `ThirdParty\QuickTime` (offline, uses `QuickTimeInstallCache.qdat`)

`runtime/install-ilumina.sh` reproduces 1–4 without the GUI;
`runtime/install-quicktime.sh` drives step 5 with xdotool.

## Feature matrix

| Feature | Result | Notes |
|---|---|---|
| Projector start, splash, Home | ✅ | |
| Registration dialog (Mui Dialog) | ✅ | Needs a press/release with a short gap; instant synthetic clicks are ignored |
| Activation dialog | ⚠️ | Works, but activation needs `visual-book.com`, which is gone. Trial mode: 15 days. See "Activation" below |
| Bible module: book tree (TreeView Xtra), chapter text | ✅ | Valentina queries work |
| Bible text formatting, links, timeline | ✅ | |
| Multimedia → Animations (QuickTime .mov) | ✅ | Plays with captions; audio reaches the browser |
| Multimedia → Virtual tours (QTVR) | ✅ | Drag-to-pan works in the browser |
| Audio (MP3 narration, QuickTime sound) | ✅ | Wine → PulseAudio → Xpra (Opus/WebM) → browser |
| Top-bar dropdown menus | ⚠️ | Open fine, but picking an item **sometimes** raises a Lingo `Index out of range` script error (click "Yes/Sim" to continue). Seen with pt_BR, C and en_US locales. The module buttons on the Home and hub pages work reliably |
| Encyclopedia, Time travel, Extras, Personal study | ⏳ | Not tested yet |
| PDFs (Impressario Xtra) | ⏳ | Not tested yet |
| Printing (PrintOMatic) | ⏳ | Disabled in the streaming session |
| Network features (NetLingo, registration post) | ❌ expected | Servers no longer exist; the app records `Prefs\NetworkError.txt` |

## Known issues / notes

- **Original vendor guidance** (see [history.md](history.md)): QuickTime 7
  breaks multi-node QTVR, so stay on 6.x. Animations are Sorenson Video and
  *Excursões Virtuais* are MPEG-4. On slow CPUs, frame drops with audio kept
  in sync are the original behaviour.

- **Intermittent menu script error.** Choosing an entry from the top-bar
  dropdowns intermittently raises `Index out of range`. It was not seen in
  a bare-Wine session (no virtual desktop, no Xpra), so suspects are the
  Wine virtual desktop and synthetic input timing. Not caused by the locale:
  it reproduced under pt_BR, C and en_US. Next step: reproduce outside Xpra
  with `WINEDEBUG=+relay` around the OSControl / Buddy API Xtras.
- **Off-screen wizard windows.** The QuickTime installer opens centred on a
  1×1 parent window, i.e. at negative coordinates; it is moved on-screen
  before clicking. The wizard retitles its window on every page, so windows
  are matched by size, not title.
- **Wine virtual desktop.** The app runs inside a 1024×768 Wine desktop (the
  size of the Director stage). That keeps every dialog inside one window in
  the browser and avoids the "your resolution is higher than needed" prompt.
- **Xpra 3.1 video encoders.** Ubuntu's Xpra 3.1.5 can't use its ffmpeg
  encoders (`all the codecs have failed`), so it falls back to
  JPEG/PNG/WebP. That's good enough for a 1024×768 stage but uses more
  bandwidth during video. Xpra ≥ 5 (from xpra.org) would add H.264/VP8.
- **Apple Silicon.** Wine needs x86. On an M-series Mac, Docker runs the
  image under emulation (`platform: linux/amd64`), which is slow. An x86_64
  Linux host or VPS is the intended target.

## Activation

iLumina Gold requires online activation (serial number → activation code
from `visual-book.com/registro.php`). That service is gone, so unactivated
copies run as a 15-day trial. This project does **not** bypass the
activation, and its persistent `/state` volume keeps the trial clock like
a normal install. Options to look into:

- an owner who still has a valid activation code can enter it ("Possuo o meu
  Código de Ativação"), by hand or through `ILUMINA_ACTIVATION_CODE` in
  `.env` (`runtime/auto-activate.sh`). Codes are issued per installation:
  the phone-activation screen shows a "#1" install code (e.g.
  `50726-…-6Z`) and asks for "#2" = the registration e-mail. The "#1" code
  is created on first launch and kept in the prefix (it did not change
  across restarts or hostnames), so a code from another PC/install is
  normally rejected: "Continuar" stays disabled;
- ask the rights holders (iLúmina Criação, Comercialização e Distribuição de
  Softwares Ltda / Visual Book) for a preservation licence, a universal
  code or permission to publish;
- check the preservation exceptions in the relevant jurisdiction before
  doing anything else.
