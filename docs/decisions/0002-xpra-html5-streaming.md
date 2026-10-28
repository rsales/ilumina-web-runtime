# 0002 — Stream with Xpra's HTML5 client from the Ubuntu archive

- Status: Accepted
- Date: 2026-10-28

## Problem
The user must see, hear and control the app from a browser tab. Narration
and video soundtracks are a core part of the content, so audio is
mandatory.

## Decision
Xpra 3.1.5 (Ubuntu 24.04 package, which includes the HTML5 client), with
PulseAudio forwarding (Opus/WebM). The app runs inside a 1024×768 Wine
virtual desktop, so the browser shows a single window.

## Alternatives considered
- **x11vnc + noVNC**: simple and robust, but no audio. We would need a
  second audio pipeline.
- **Selkies / WebRTC**: better latency and codecs, but heavier and not in
  the Ubuntu archive.
- **Xpra from xpra.org (v5/6)**: working H.264/VP8 encoders, but a
  third-party repo. Revisit when smoother video matters.

## Consequences
- ✅ Picture, input, clipboard and sound over one WebSocket on port 14500.
- ❌ Ubuntu's Xpra 3.1 can't use its ffmpeg encoders ("all the codecs have
  failed"), so it falls back to JPEG/PNG/WebP. Fine at 1024×768, but heavy
  during video.
- ⚠️ Xpra only maps windows and delivers synthetic input **while a client
  is connected**. Any automation in the streaming session must wait for a
  client (see `runtime/auto-activate.sh`).
- The package needs `libjs-jquery` (a dangling symlink otherwise), and the
  default `start = /etc/X11/Xsession true` must be disabled.

## Evidence
Browser test with Playwright/Chromium: video playing, QTVR panning, and
`audio-state: playing` with an Opus stream in the Xpra log.
