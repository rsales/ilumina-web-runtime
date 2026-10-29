#!/usr/bin/env python3
"""Cap the audio latency of Ubuntu's Xpra 3.1 HTML5 client.

The 3.1 client appends every audio chunk to a MediaSource buffer and never
skips ahead. Each stall (emulator hiccup, background tab, network) adds to
the delay for good: sound starts late and keeps playing after the video is
paused. This patch jumps playback to near the live edge whenever more than
AUDIO_MAX_LATENCY_MS is buffered. A short skip is less noticeable than an
ever-growing lag.

Defaults 2000/800 ms: Xpra delivers audio in bursts and the playable buffer
normally swings between 0.3 and 1.5 s. A 500/150 cap (first attempt) seeked
on almost every burst and starved playback, which ended up silent.

Tunable per session with the URL params ?audio_max_latency=ms and
?audio_target_latency=ms. 0 disables the cap.
Usage: patch-xpra-html5-audio.py /usr/share/xpra/www/js/Client.js
"""
import sys

path = sys.argv[1]
src = open(path, encoding="utf-8").read()

MARK = "/* ilumina: audio latency cap */"
if MARK in src:
    sys.exit(0)

ANCHOR = '''			var buf_size = Math.round(1000*(e - this.audio.currentTime));
			this.debug("audio", "buffer size=", buf_size, "ms, currentTime=", this.audio.currentTime);
'''
if ANCHOR not in src:
    sys.exit("patch-xpra-html5-audio: anchor not found (Xpra version changed?)")

PATCH = ANCHOR + '''			''' + MARK + '''
			var maxl = parseInt(Utilities.getparam("audio_max_latency") || "2000", 10);
			var target = parseInt(Utilities.getparam("audio_target_latency") || "800", 10);
			var live = b.end(b.length-1);
			var lag = Math.round(1000*(live - this.audio.currentTime));
			if (maxl > 0 && lag > maxl && !this.audio.paused) {
				this.clog("audio lag", lag, "ms > ", maxl, "ms, skipping to", target, "ms behind live");
				this.audio.currentTime = Math.max(b.start(b.length-1), live - target/1000);
			}
'''
src = src.replace(ANCHOR, PATCH, 1)
open(path, "w", encoding="utf-8").write(src)
print("patched", path)
