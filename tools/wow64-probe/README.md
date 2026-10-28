# WoW64 probes

**Question these answer:** can this machine run 32-bit Windows programs
through Wine's new WoW64? (ADR 0006)

| Probe | What it isolates | Expected on x86_64 |
|---|---|---|
| `compat32` (Linux ELF, static) | the CPU/emulator switching a 64-bit process into 32-bit compatibility mode (CS=0x23), **without Wine**. That switch is what WoW64 does for every 32-bit call | `COMPAT32-OK`, exit 0 |
| `wine cmd` | Wine itself, 64-bit only | `WINE64-OK` |
| `hello32.exe` (PE32, kernel32 only, no CRT) | the smallest possible 32-bit Windows program under WoW64 | `WOW64-32BIT-OK`, exit 0 |

How to read the results:
- `compat32` segfaults (exit 139): the environment can't run 32-bit code
  inside a 64-bit process, so **no** WoW64 Wine can work there.
- `compat32` OK but `hello32.exe` fails: the problem is in Wine or the prefix.
- Everything OK but iLumina fails: the problem is app-specific.

Run (from the repo root, on the experiment branch):

```sh
docker compose run --rm -v "$PWD/tools/wow64-probe:/opt/ilumina-probe:ro" \
  ilumina bash /opt/ilumina-probe/run-probes.sh
```

Build (prebuilt binaries are committed; they are our own code):

```sh
gcc -O0 -static -no-pie -o compat32 compat32.c
i686-w64-mingw32-gcc -O1 -nostdlib -e _start@0 -Wl,--subsystem,console -o hello32.exe hello32.c -lkernel32
```

Verified 2026-10-28 on x86_64 (sandbox): all three OK.
