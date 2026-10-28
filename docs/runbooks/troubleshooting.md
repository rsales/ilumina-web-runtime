# Troubleshooting

Symptom first. Each entry says how to confirm the cause and what to do.

| Symptom | Likely cause | Confirm | Fix |
|---|---|---|---|
| `missing /media/ilumina/cdN` | wrong `ILUMINA_MEDIA_DIR` or layout | `ls $ILUMINA_MEDIA_DIR` | point it at the folder that contains `cd1..cd4` |
| `cdN is not readable by uid 1000` | host files are mode 600/700 | `ls -l discs/cd1` | `chmod -R a+rX discs` |
| `cdN does not look like iLumina CD N` | extracted one level too high or low | look for `N.txt` | move the folder that contains `N.txt` to `cdN` |
| `QuickTime wizard window not found (page N)` | Wine rendering changed the window size | `/state/logs/screens/qt-page-N.png` | re-measure: [debugging-gui-automation.md](debugging-gui-automation.md) |
| Browser shows a blue/blank page, no window | Xpra HTML5 assets missing | browser console: 404 on `js/lib/jquery.js` | make sure `libjs-jquery` is in the image |
| Browser connected, but the app is not visible | app still starting, or a dialog is waiting | `docker compose exec ilumina pgrep -a iLumina` | wait 20–30 s; check `/state/logs/ilumina.log` |
| "Director Player Error: Index out of range" | known intermittent bug, top-bar menus | — | click "Yes"; use the Home/hub buttons. Tracked in compatibility.md |
| "Network Performance Issue" toast | Xpra 3.1 has no H.264, so frames are JPEG/WebP | — | click "Ignore", or lower the bandwidth limit; see decision 0002 |
| Activation code rejected ("Continuar" stays grey) | code was issued for another install's "#1" code | `/state/logs/auto-activate.log` | see use-cases UC4. Do not try to work around it (decision 0005) |
| `failed to create main module … status c0000018` (Mac M) | the exe needs fixed address 0x400000, taken by the x86 emulator | log right after "running QuickTime" | not fixable in this container on Apple Silicon (32-bit x86 runs under QEMU user-mode, with or without Rosetta). Run on an x86_64 host or a full x86 VM |
| `rosetta error: invalid gdt selector index 4` (Mac M) | Rosetta for Linux has no 32-bit compatibility mode | `tools/wow64-probe` on branch `experiment/wine11-wow64` | not fixable in a container; use an x86_64 host |
| Very slow on a Mac | x86 emulation on Apple Silicon | `uname -m` in the container shows `x86_64` | run on an x86_64 host |
