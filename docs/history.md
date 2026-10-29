# History and original vendor guidance

What the publisher said about iLúmina Gold when it was on sale. This page
explains *why* the runtime is built the way it is (QuickTime 6.3, 1024×768,
4 discs) and gives the hardware baseline that emulation has to beat.

Everything here is paraphrased from the publisher's site as archived by the
Wayback Machine. Read the originals there rather than copying them into this
repo (ADR 0003):

- <https://web.archive.org/web/20051214200423/http://www.iluminabrasil.com.br/home/index.htm>
- <https://web.archive.org/web/20080809024003/http://www.iluminabrasil.com.br/home/index.htm>

## The product

| Item | Fact (source: 2005 press release on the archived site) |
|---|---|
| Publisher | Sociedade Bíblica do Brasil (SBB), developed by iLúmina Brasil |
| Launch | XII Bienal Internacional do Livro do Rio de Janeiro, 21 May (2005), Riocentro |
| Launch price | R$ 129,00 |
| Pitch | "first Bible with a digitally animated encyclopedia" |
| Content | Bible in Almeida RA and RC, NTLH, King James (English); cross references and footnotes; encyclopedia with hundreds of articles; animations of Bible stories (scene capture), virtual tours of the Holy Land and the Jerusalem Temple, video documentaries, image export, a presentation builder |
| Lineage | The FAQ dates support for the original (US) iLumina from **September 2002** |
| Sister products | *A Vida de Jesus* on DVD and VHS (2 tapes, 12 animations) |

## Original system requirements

| | Windows | Mac |
|---|---|---|
| CPU | Pentium III 500 MHz or faster | — (Mac OS X 10.2 or later) |
| OS | Windows 98, 2000 Professional, Me, XP (NT not supported) | Mac OS X 10.2+ |
| RAM | 128 MB minimum, 256 MB recommended (the FAQ says 256 MB for both) | 256 MB |
| Disk | 2 GB for a full install | 2 GB |
| Display | 1024×768, 16-bit colour or more | same |
| Other | 4× CD-ROM, sound card, QuickTime 6.3 (on the discs), Acrobat 6 (included), printer and internet optional | same |

**Why it matters here:** the emulated machine only has to match a 2005
Pentium III 500 MHz with 256 MB of RAM. That's the bar for ADR 0007's L7
performance layer.

## What the publisher's FAQ tells us about media

These are the support answers that bear on this project, in short.

1. **QuickTime 6.3 is required** for animations, virtual tours (*Passeios
   Virtuais*), *Momentos no Tempo* and *Excursões Virtuais*. A "broken film"
   icon in the media window means QuickTime is missing or too old. The
   installer asks for a registration number: leave it empty and press Next.
2. **QuickTime 6.5.2 was the official fallback** when 6.3 misbehaved on a
   machine.
3. **QuickTime 7 is incompatible** (FAQ E.17): it doesn't play
   **multi-node QTVR**, so the virtual tours break, and on some machines the
   animations do too. The fix was to uninstall 7 and go back to 6.3, or 6.5
   if needed. → Never "upgrade" QuickTime in the runtime.
4. **Codecs:** animations use **Sorenson Video**; the *Excursões Virtuais*
   videos use **MPEG-4**. Both are why the FAQ asked users to update
   QuickTime and video drivers.
5. **Slow CPUs drop frames on purpose.** On machines near the minimum spec
   the codec skips frames, so animations look like slow motion **while
   sound and picture stay in sync**. This may or may not happen for the
   *Excursões Virtuais* videos. → Under emulation, choppy video is expected
   behaviour of the original; **choppy or broken audio is not**.
6. **Stuttering sound or garbled video** were, in the publisher's
   experience, almost always an outdated OS, QuickTime or video driver.
7. **"File not found under the install folder"** in a media window means an
   incomplete install. Media lives on CDs 2–4 under
   `Files/Media/{Animations,GuidedTours,Mp3,PDFs,QTVRs,Videos}`. Fix: copy
   the file from the disc or reinstall all four discs. → Matches
   `runtime/install-ilumina.sh` and the SHA-256 manifest.
8. **Media players could hijack playback.** RealPlayer was the known
   culprit: it took over the `.mov` association and the media stopped
   playing. Not relevant inside a clean Wine prefix.
9. **Mac:** the drive doesn't eject on its own between discs; use the Eject
   key (or F12).

## Consequences for this project

| Finding | Where it shows up |
|---|---|
| QuickTime 6.x only, never 7 | `runtime/install-quicktime.sh` installs the bundled 6.3 (ADR 0004); Boxedwine `web/scripts/make-overlay.sh` → `run.bat` |
| 1024×768 stage | Wine virtual desktop size, Boxedwine `-resolution 1024x768` |
| PIII-500 / 256 MB baseline | L7 performance layer (ADR 0007, on `experiment/boxedwine`) |
| Sorenson + MPEG-4 + multi-node QTVR | What any non-QuickTime path (ADR 0007's "redirect to a Director player" outcome) would have to decode |
| Frame dropping is by design | Judge emulated video by A/V sync, not frame rate. Audio must not stutter (open issue on Boxedwine native, `experiment/boxedwine`) |
| A Mac OS X 10.2 build exists on the hybrid discs | A possible preservation path (PowerPC emulation) not explored: ⏳ |
