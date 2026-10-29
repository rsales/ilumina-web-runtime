# Boxedwine test matrix (ADR 0007)

Fill one column per environment. Use ✅ / ⚠️ (works with issues: describe) /
❌ (fails: paste the first error from the browser console) / ⏳ (not run).
Stop at the first ❌: the layers after it are not conclusive.

| Layer | What to check | native x86_64 | native Mac M4 (arm64) | wasm Chrome Mac M4 |
|---|---|---|---|---|
| L1 emulator | `hello32gui.exe` message box; `C:\boxedwine-probe.txt` written | ⏳ | ⏳ | ⏳ |
| L2 Boxedwine's Wine | notepad opens and accepts typing | ⏳ | ⏳ | ⏳ |
| L3 Director | `iLumina.exe` splash ("CARREGANDO…"), registration dialog | ⏳ | ⏳ | ⏳ |
| L4 content | Home screen, Bíblia → Gênesis text | ⏳ | ⏳ | ⏳ |
| L5 Xtras | book tree (TreeView), Bible text (Valentina), Mui dialogs | ⏳ | ⏳ | ⏳ |
| L6 QuickTime | 6.3 wizard completes; Animações plays with sound; QTVR pans | ⏳ | ⏳ | ⏳ |
| L7 performance | time to Home (s); tab memory (GB); video fps / QTVR smoothness | — | — | ⏳ |

## Environment details (fill in)

- Boxedwine commit: `8790ff20428c22daa64368af9553417e4f499c94`, target `multiThreadedJit`
- File system: `TinyCore15Wine11.0.zip` (Wine 11.0, 32-bit)
- Browser + version:
- Overlay size: ilumina.zip = 1.9 GB (full install layout)

## Notes / errors
