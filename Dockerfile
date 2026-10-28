# iLumina Web Runtime
#
# Runs the original Windows build of iLumina Gold (Macromedia Director MX
# projector + Xtras + QuickTime 6.3) under Wine, and streams it to the
# browser with the Xpra HTML5 client (video + audio).
#
# This image contains NO iLumina content. The original CDs are mounted at
# runtime (see compose.yaml) and installed into a persistent Wine prefix on
# first start — the same "bring your own copy" model used by nfsu2-web.
#
# Everything comes from the Ubuntu archive so the build is reproducible
# without third-party repositories.

FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# Wine 9.0 (32-bit prefix), Xpra 3.1 (includes the HTML5 client), PulseAudio
# for sound forwarding, xdotool/imagemagick for the unattended installers.
RUN printf '%s\n' \
      'Types: deb' \
      'URIs: http://archive.ubuntu.com/ubuntu' \
      'Suites: noble noble-updates noble-security' \
      'Components: main restricted universe multiverse' \
      'Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg' \
      > /etc/apt/sources.list.d/ubuntu.sources \
 && rm -f /etc/apt/sources.list \
 && dpkg --add-architecture i386 \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates tini procps \
      wine wine32:i386 wine64 \
      xvfb xauth x11-utils x11-xserver-utils xdotool imagemagick \
      xpra libjs-jquery python3-pil python3-numpy python3-lz4 python3-xdg \
      python3-pyinotify python3-gi python3-gi-cairo python3-cairo gir1.2-gtk-3.0 \
      python3-gst-1.0 gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-pulseaudio gstreamer1.0-tools \
      pulseaudio pulseaudio-utils libpulse0:i386 libasound2-plugins:i386 \
      fonts-liberation fonts-dejavu-core \
 && sed -i 's|^start = /etc/X11/Xsession true|# start = (disabled: no desktop session needed)|' \
      /etc/xpra/conf.d/60_server.conf \
 && rm -rf /var/lib/apt/lists/*

# Wine derives the Windows locale from LANG; C.UTF-8 maps to en-US with the
# cp1252 ANSI codepage, which renders the Portuguese text correctly.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    WINEARCH=win32 \
    WINEPREFIX=/state/prefix \
    WINEDEBUG=-all \
    ILUMINA_MEDIA=/media/ilumina \
    ILUMINA_RES=1024x768 \
    XPRA_PORT=14500

RUN useradd --create-home --uid 1000 --shell /bin/bash ilumina \
 && mkdir -p /state /media/ilumina \
 && chown ilumina:ilumina /state

COPY runtime/ /opt/ilumina/
RUN chmod +x /opt/ilumina/*.sh

USER ilumina
WORKDIR /home/ilumina
VOLUME ["/state"]
EXPOSE 14500

HEALTHCHECK --interval=30s --timeout=5s --start-period=10m \
  CMD pgrep -f 'iLumina.exe' >/dev/null || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/opt/ilumina/entrypoint.sh"]
CMD ["run"]
