# Run iLumina locally

**Goal:** iLumina open in your browser, from your own discs.

## 1. Prepare the discs

Target layout, with one folder per disc and the marker file in its root:

```
discs/cd1/1.txt   discs/cd1/Program/iLumina.exe …
discs/cd2/2.txt   discs/cd2/Files/Media/…
discs/cd3/3.txt   discs/cd4/4.txt
```

- From images: `tools/extract-discs.sh ./discs cd1.iso cd2.nrg cd3.nrg cd4.nrg`
  (needs 7-Zip; CD 1 is a Mac/PC hybrid and the script handles the nested
  HFS+ volume).
- From physical discs or mounted images: copy each disc's root into `discs/cdN/`.

Check it:

```sh
tools/verify-discs.sh ./discs     # expect: OK: 3298 files match …
```

## 2. Configure

```sh
cp .env.example .env
# ILUMINA_MEDIA_DIR=./discs     (required)
# ILUMINA_REG_* / ILUMINA_ACTIVATION_CODE   (optional, see use-cases UC4)
```

## 3. Start

```sh
docker compose up -d --build
docker compose logs -f          # wait for "xpra is ready."
```

Open <http://localhost:14500>. The first start installs first, which takes
2–5 minutes.

## 4. Everyday commands

| Task | Command |
|---|---|
| Stop | `docker compose down` |
| Reinstall from scratch (**loses notes, prefs, activation, trial state**) | `docker compose down -v` |
| Only run the installer | `docker compose run --rm ilumina install` |
| Shell with the Wine env | `docker compose run --rm ilumina shell` |
| Read logs / installer screenshots | `docker compose exec ilumina ls /state/logs /state/logs/screens` |

## Validation

- `docker compose ps` shows `healthy` (the healthcheck looks for `iLumina.exe`).
- The browser shows the Home screen, and the speaker icon in the Xpra
  toolbar is not crossed out.
- Bible → Gênesis shows the text. Multimídia → Animações plays with sound.
