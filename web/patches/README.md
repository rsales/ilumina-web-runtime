# Patches applied on top of the pinned Boxedwine commit

| Patch | Why | Upstream |
|---|---|---|
| `0001-xkeyboard-shift-2-is-at.patch` | US keymap maps Shift+2 to `&` instead of `@` (`XK_ampersand` vs `XK_at` in `source/x11/xkeyboard.cpp`), so **e-mail addresses can't be typed** and iLumina's registration dialog rejects the form ("Ponha um endereço de email válido"). Found 2026-10-29 during test layer L3. | to report |
| `0002-native-audio-buffer-knobs.patch` | Native SDL audio queues at most 125 ms and Wine's OSS driver sees every queued byte, so it writes only 1–2 periods ahead and **audio stutters** whenever the emulator or the Xpra encoder stalls (seen on Mac M4, 2026-10-29). Adds opt-in env vars `BOXEDWINE_AUDIO_BUFFER_MS`, `BOXEDWINE_AUDIO_RESERVE_MS`, `BOXEDWINE_AUDIO_PERIOD_FRAMES`; unset = upstream behaviour. Mirrors the reserve the web build already uses. | to report after retest |
