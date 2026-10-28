# Architecture notes

## Why Wine and not static recompilation

nfsu2-web lifts the game's x86 code to C and compiles it to WebAssembly.
That works because the game *is* its executable. iLumina's `iLumina.exe` is
a generic Director MX projector. The application itself is:

- Lingo bytecode inside protected movies (`Home.dxr`, `Bible.dxr`, `GCode.cxt`, …)
- 30+ native Xtras (`Xtras/Win/*.x32`), several of them third-party
  (Valentina, BuddyAPI, vList, TreeView, Impressario, PrintOMatic)
- QuickTime 6 for every video and QTVR panorama

Recompiling the projector would only give you the Director player. Every
Xtra, and QuickTime itself, would still need a port. Running everything
unmodified under Wine keeps the original behaviour and is the fastest way
to get to a playable archive.

Longer term, a native browser version could use a Director runtime in
WebAssembly (DirPlayer, ScummVM's Director engine) with reimplemented
Xtras: Valentina → SQLite, QuickTime → MP4, QTVR → a WebGL panorama viewer,
Impressario → PDF.js. The compatibility work done here (file formats, Xtra
inventory, the `iLuminaInfo.txt` contract) carries over.

## Components

| Layer | Choice | Why |
|---|---|---|
| Windows API | Wine 9.0, 32-bit prefix | Director MX and its Xtras are Win32. Wine handles Director, Valentina and QuickTime 6.3 well |
| Display | Xvfb inside Xpra, Wine virtual desktop 1024×768 | One window per session. The Director stage is exactly 1024×768 |
| Streaming | Xpra 3.1 HTML5 client | Video, keyboard/mouse, clipboard and **audio** over one WebSocket; packaged in Ubuntu |
| Audio | PulseAudio null sink → Xpra → Opus/WebM | Wine's winepulse driver plays into Xpra's sink |
| State | Docker volume `/state` (Wine prefix, prefs, logs) | Persistent like a real install, including user notes and favourites |
| Media | Read-only bind mount `/media/ilumina/cd1..cd4` | Bring your own discs |

## Install flow (first start)

1. `check_media`: each `cdN/N.txt` exists, and `cd1/Program/iLumina.exe` exists
2. `wineboot`, then map D:–G: → cd1–cd4 as CD-ROM drives
3. Copy the program and merge the media (the same files the original installer copies)
4. Write `iLuminaInfo.txt` and the Uninstall registry key
5. Run the QuickTime 6.3 installer on a temporary Xvfb, driven by xdotool
   (windows matched by size; a screenshot of every step is saved)
6. Write the `/state/.installed` marker

## Multi-user hosting (next step)

One Wine session can't be shared, so public hosting needs one container per
visitor:

- a small gateway hands each visitor a fresh container from a pre-installed
  "golden" `/state` snapshot (copy-on-write: overlayfs or a btrfs/zfs clone);
- idle sessions are reaped after N minutes;
- each session needs about 1 vCPU and 512 MB RAM.

Pick the snapshot and persistence policy carefully. Do **not** design it to
reset the product's activation trial (see COMPATIBILITY.md → Activation).
