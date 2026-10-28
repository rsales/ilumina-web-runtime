# iLumina Web Runtime
#
# EXPERIMENTAL BRANCH: Wine 11 new-WoW64 (see ADR 0006).
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

# EXPERIMENT (branch experiment/wine11-wow64):
# Wine 11 from WineHQ in "new WoW64" mode. 32-bit Windows code runs inside a
# 64-bit Linux process, so on Apple Silicon Docker hands the whole thing to
# Rosetta (x86_64) instead of QEMU user-mode (i386). QEMU user-mode is what
# broke QuickTimeInstaller.exe there with status c0000018.
# See docs/decisions/0006-experiment-wine11-wow64.md.
ARG WINE_BRANCH=stable
RUN printf '%s\n' \
      'Types: deb' \
      'URIs: http://archive.ubuntu.com/ubuntu' \
      'Suites: noble noble-updates noble-security' \
      'Components: main restricted universe multiverse' \
      'Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg' \
      > /etc/apt/sources.list.d/ubuntu.sources \
 && rm -f /etc/apt/sources.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates wget gnupg \
 && mkdir -pm755 /etc/apt/keyrings \
 && wget -qO /etc/apt/keyrings/winehq-archive.key https://dl.winehq.org/wine-builds/winehq.key \
 && wget -qNP /etc/apt/sources.list.d/ https://dl.winehq.org/wine-builds/ubuntu/dists/noble/winehq-noble.sources \
 && dpkg --add-architecture i386 \
 && apt-get update \
 && apt-get install -y --install-recommends "winehq-${WINE_BRANCH}" \
 && apt-get install -y --no-install-recommends \
      tini procps \
      xvfb xauth x11-utils x11-xserver-utils xdotool imagemagick file \
      xpra libjs-jquery python3-pil python3-numpy python3-lz4 python3-xdg \
      python3-pyinotify python3-gi python3-gi-cairo python3-cairo gir1.2-gtk-3.0 \
      python3-gst-1.0 gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-pulseaudio gstreamer1.0-tools \
      pulseaudio pulseaudio-utils \
      fonts-liberation fonts-dejavu-core \
 && sed -i 's|^start = /etc/X11/Xsession true|# start = (disabled: no desktop session needed)|' \
      /etc/xpra/conf.d/60_server.conf \
 && rm -rf /var/lib/apt/lists/*

# Force new WoW64: hide the i386 *Unix* side of Wine, so 32-bit programs can
# only run through the 64-bit process. The 32-bit Windows (PE) DLLs stay.
# This fails the build loudly if the package layout is not what we expect.
RUN W=/opt/wine-${WINE_BRANCH}/lib/wine \
 && echo "Wine layout:" && ls "$W" \
 && test -d "$W/x86_64-unix" && test -d "$W/i386-windows" \
 && test -f "$W/x86_64-windows/wow64.dll" && test -f "$W/x86_64-windows/wow64cpu.dll" \
 && if [ -d "$W/i386-unix" ]; then mv "$W/i386-unix" "$W/i386-unix.disabled"; fi \
 && /opt/wine-${WINE_BRANCH}/bin/wine --version

# Wine derives the Windows locale from LANG; C.UTF-8 maps to en-US with the
# cp1252 ANSI codepage, which renders the Portuguese text correctly.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    PATH=/opt/wine-stable/bin:$PATH \
    WINEARCH=win64 \
    WINEPREFIX=/state/prefix \
    WINEDEBUG=-all \
    ILUMINA_MEDIA=/media/ilumina \
    ILUMINA_RES=1024x768 \
    XPRA_PORT=14500

# The official ubuntu:24.04 image already ships a "ubuntu" user with UID 1000.
RUN (userdel -r ubuntu 2>/dev/null || true) \
 && useradd --create-home --uid 1000 --shell /bin/bash ilumina \
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
