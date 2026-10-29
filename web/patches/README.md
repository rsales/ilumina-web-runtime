# Patches applied on top of the pinned Boxedwine commit

| Patch | Why | Upstream |
|---|---|---|
| `0001-xkeyboard-shift-2-is-at.patch` | US keymap maps Shift+2 to `&` instead of `@` (`XK_ampersand` vs `XK_at` in `source/x11/xkeyboard.cpp`), so **e-mail addresses can't be typed** and iLumina's registration dialog rejects the form ("Ponha um endereço de email válido"). Found 2026-10-29 during test layer L3. | to report |
