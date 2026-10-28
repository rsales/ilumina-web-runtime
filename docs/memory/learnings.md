# Learnings

Append new entries at the top. Format: `YYYY-MM-DD — fact. Evidence. (→ promoted to X)`.

- 2026-10-28 — The official `ubuntu:24.04` image has a `ubuntu` user with UID 1000;
  a locally debootstrapped base doesn't, so the sandbox build hid the
  `useradd: UID 1000 is not unique` failure. Test with the real base image
  when you can. (→ Dockerfile)

- 2026-10-28 — Xpra delivers no synthetic input to app windows while no client
  is connected. Clicks from auto-activate.sh did nothing until a browser
  joined. (→ promoted to decision 0002)
- 2026-10-28 — Inside the Wine virtual desktop, `xdotool getwindowgeometry`
  reports doubled child positions (dialog at 190,140 reported as 378,280).
  Use absolute desktop coordinates. (→ runbooks/debugging-gui-automation.md)
- 2026-10-28 — The iLumina install code "#1" stayed the same across container
  restarts and hostname changes, so it lives in the prefix. It is created on
  first launch. (→ use-cases UC4)
- 2026-10-28 — The `Index out of range` menu error is NOT locale-related:
  reproduced with pt_BR, C and en_US. One en_US run without the error was a
  false lead. (→ compatibility.md)
- 2026-10-28 — The QuickTime 6.3 wizard opens centred on a 1×1 parent (at
  negative coordinates) and renames its window on every page.
  (→ decision 0004)
- 2026-10-28 — Director Mui dialogs ignore `xdotool click`. Press and release
  with a gap of at least 150 ms. (→ runtime/lib.sh slow_click)
- 2026-10-28 — Ubuntu's xpra 3.1.5 needs `libjs-jquery`, or the HTML5 client
  fails with "jQuery is not defined". (→ Dockerfile)
- 2026-10-28 — The sandbox egress blocks Docker Hub. The `ubuntu:24.04` base
  was built locally with `debootstrap` + `docker import` for testing.
  Normal machines just pull it.
