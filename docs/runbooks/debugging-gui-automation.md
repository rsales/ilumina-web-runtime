# Debugging the GUI automation

**Goal:** fix `install-quicktime.sh` or `auto-activate.sh` when clicks no
longer land, for example after a Wine upgrade.

## How the automation finds things

- Wizard windows are matched by **exact size** (`find_window_sized W H` in
  `runtime/lib.sh`), not by title, because the QuickTime wizard retitles
  itself on every page.
- Windows that open at negative coordinates are moved to (200,150) first
  (`ensure_onscreen`).
- Buttons are clicked at a fixed offset from the window's corner with
  `slow_click` (press, 150 ms, release). Director's Mui dialogs ignore
  instant clicks.
- Inside the streaming session, coordinates are **absolute in the 1024×768
  Wine desktop**. `xdotool getwindowgeometry` reports wrong positions for
  windows inside the Wine desktop, so don't use it there.
- In the streaming session, nothing receives input until a browser is
  connected (`xpra info :100` → `clients=1`).

## Re-measuring

```sh
docker compose run --rm ilumina shell
source /opt/ilumina/lib.sh; start_setup_display
(cd "$APP_DIR/ThirdParty/QuickTime" && wine QuickTimeInstaller.exe &) ; sleep 15
for id in $(xdotool search --onlyvisible --name ''); do
  echo "$id [$(xdotool getwindowname $id)] $(xdotool getwindowgeometry --shell $id | tr '\n' ' ')"
done
import -window root /state/logs/screens/measure.png   # open it, read the pixel positions
```

Update `WIZARD_SIZE/WIZARD_BTN` or `ASSIST_SIZE/ASSIST_BTN`, then validate
on a **fresh** volume:

```sh
docker volume rm ilumina-web-runtime_ilumina-state   # check the name with: docker volume ls
docker compose run --rm ilumina install          # must end with "installation complete"
```

Write what you measured in `docs/memory/learnings.md`.
