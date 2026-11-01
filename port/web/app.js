// iLumina port: browser shell over the `web` crate's wasm ABI (no bindgen).
// Files come from the local server (`xtask serve`), which only serves paths
// listed in the release manifest; the wasm side checks every hash again.

const MOVIES = ["Home", "Bible", "Encyclopedia", "TimeTravel", "MediaCenter",
  "Extras", "MyStudyCenter", "SearchResultsPanel"];
const DIR = "cd1/Program/Files";
const PROJECTOR = "cd1/Program/iLumina.exe";
const GLYPH_MAX = 256;

const $ = (id) => document.getElementById(id);
const enc = new TextEncoder();
const dec = new TextDecoder();
let wasm;
const added = new Set();

const mem = () => new Uint8Array(wasm.memory.buffer);

function putStr(s) {
  const b = enc.encode(s);
  const p = wasm.alloc(b.length);
  mem().set(b, p);
  return [p, b.length];
}

function result() {
  const p = wasm.result_ptr();
  return dec.decode(mem().slice(p, p + wasm.result_len()));
}

/** Call an export that takes one string; throws with the engine's message. */
function callStr(name, s) {
  const [p, l] = putStr(s);
  const r = wasm[name](p, l);
  wasm.dealloc(p, l);
  if (r < 0) throw new Error(result());
  return r;
}

// --- fonts: the browser rasterises each glyph once (the engine caches) ---

const FAMILY = {
  "geneva": "Verdana",
  "charcoal": "Verdana",
  "ms sans serif": '"Microsoft Sans Serif", Tahoma, Arial',
  "times": '"Times New Roman", Times',
  "helvetica": "Helvetica, Arial",
};

function cssFont(family, size, flags) {
  const f = FAMILY[family.toLowerCase()] ?? `"${family}"`;
  return `${flags & 2 ? "italic " : ""}${flags & 1 ? "bold " : ""}${size}px ${f}, sans-serif`;
}

const gcanvas = typeof OffscreenCanvas !== "undefined"
  ? new OffscreenCanvas(GLYPH_MAX, GLYPH_MAX)
  : Object.assign(document.createElement("canvas"), { width: GLYPH_MAX, height: GLYPH_MAX });
const g = gcanvas.getContext("2d", { willReadFrequently: true });

function js_glyph(fp, fl, size, flags, ch, outPtr, buf, cap) {
  const family = dec.decode(mem().slice(fp, fp + fl));
  g.font = cssFont(family, size, flags);
  g.textBaseline = "alphabetic";
  const s = String.fromCodePoint(ch);
  const m = g.measureText(s);
  const left = Math.ceil(m.actualBoundingBoxLeft);
  const right = Math.ceil(m.actualBoundingBoxRight);
  const asc = Math.ceil(m.actualBoundingBoxAscent);
  const desc = Math.ceil(m.actualBoundingBoxDescent);
  let w = left + right, h = asc + desc;
  if (w <= 0 || h <= 0 || w > GLYPH_MAX || h > GLYPH_MAX || w * h > cap) { w = 0; h = 0; }
  if (w > 0) {
    g.clearRect(0, 0, w, h);
    g.fillStyle = "#000";
    g.fillText(s, left, asc);
    const px = g.getImageData(0, 0, w, h).data;
    const cov = new Uint8Array(w * h);
    for (let i = 0; i < cov.length; i++) cov[i] = px[i * 4 + 3];
    mem().set(cov, buf);
  }
  const out = new Int32Array(wasm.memory.buffer, outPtr, 5);
  out.set([w, h, -left, -desc, Math.round(m.width * 64)]);
}

function js_line(fp, fl, size, flags, outPtr) {
  const family = dec.decode(mem().slice(fp, fp + fl));
  g.font = cssFont(family, size, flags);
  const m = g.measureText("Hg");
  const out = new Int32Array(wasm.memory.buffer, outPtr, 3);
  out.set([Math.round(m.fontBoundingBoxAscent * 64), Math.round(m.fontBoundingBoxDescent * 64), 0]);
}

// --- files ---

async function addFile(rel) {
  if (added.has(rel)) return;
  const url = "discs/" + rel.split("/").map(encodeURIComponent).join("/");
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${rel}: HTTP ${r.status}`);
  const bytes = new Uint8Array(await r.arrayBuffer());
  sizes.set(rel, bytes.length);
  const p = wasm.alloc(bytes.length);
  mem().set(bytes, p);
  const [sp, sl] = putStr(rel);
  const rc = wasm.file_add(sp, sl, p, bytes.length); // takes ownership of p
  wasm.dealloc(sp, sl);
  if (rc < 0) throw new Error(result());
  added.add(rel);
}

/** Fetch several files (a few at a time), reporting progress. */
async function addFiles(list, label) {
  let done = 0, bytes = 0;
  const queue = list.filter((f) => !added.has(f));
  const worker = async () => {
    while (queue.length) {
      const rel = queue.shift();
      await addFile(rel);
      done++;
      bytes += sizes.get(rel) ?? 0;
      status(`${label}: ${done}/${list.length} arquivos, ${(bytes / 1048576).toFixed(0)} MB`);
    }
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
}
const sizes = new Map();

// --- UI ---

let width = 0, height = 0, frames = 0;

function status(text, error = false) {
  $("status").textContent = text;
  $("status").className = error ? "error" : "";
}

async function openMovie(name) {
  stopPlayback();
  const movie = `${DIR}/${name}.dxr`;
  status(`carregando ${name}…`);
  const t0 = performance.now();
  await addFile(PROJECTOR);
  await addFile(movie);
  const n = callStr("needed_files", movie);
  const casts = n > 0 ? result().split("\n") : [];
  for (const c of casts) await addFile(c);
  callStr("load", movie);
  width = wasm.stage_width();
  height = wasm.stage_height();
  frames = wasm.frame_count();
  const c = $("stage");
  c.width = width;
  c.height = height;
  $("frame").max = $("slider").max = Math.max(frames, 1);
  status(`${name}: ${casts.length} casts, ${frames} frames, ${Math.round(performance.now() - t0)} ms`);
  if ($("lingo").checked) {
    await play(movie);
  } else {
    draw(1);
  }
}

// --- Lingo: run the movie's startup, fetching what it reads ---

const OWNER_KEY = "ilumina-owner";

function loadOwner() {
  // Kept only in this browser (sessionStorage, or localStorage when the
  // owner asks to remember); never sent anywhere.
  const raw = sessionStorage.getItem(OWNER_KEY) ?? localStorage.getItem(OWNER_KEY);
  if (!raw) return;
  const [n, e, c] = raw.split("\n");
  $("regName").value = n ?? "";
  $("regEmail").value = e ?? "";
  $("regCep").value = c ?? "";
}

function applyOwner() {
  const v = [$("regName").value, $("regEmail").value, $("regCep").value].join("\n");
  callStr("set_owner", v);
  sessionStorage.setItem(OWNER_KEY, v);
  if ($("regRemember").checked) localStorage.setItem(OWNER_KEY, v);
  else localStorage.removeItem(OWNER_KEY);
}

async function play(movie) {
  const t0 = performance.now();
  const name = movie.split("/").pop();
  for (let round = 1; ; round++) {
    if (round > 40) throw new Error("a execução continua pedindo arquivos");
    status(`${name}: executando o Lingo (rodada ${round})…`);
    await new Promise((r) => setTimeout(r)); // let the status paint
    const n = callStr("play", movie);
    if (n === 0) break;
    await addFiles(result().split("\n"), `${name}: arquivos que o Lingo leu`);
  }
  const steps = result().split("\n");
  $("steps").textContent = steps.join("\n");
  const stop = steps.find((s) => !s.endsWith(": OK"));
  // The first-run registration dialog: the owner types their own data.
  $("register").hidden = !(stop && stop.includes("Registre Sua C"));
  await drawPlay();
  const ms = Math.round(performance.now() - t0);
  status(stop ? `${name}: parou em ${stop.split(":")[0]} (${ms} ms)` : `${name}: partida completa (${ms} ms)`, !!stop);
  if (!stop) startPlayback(movie);
}

// --- playback: one frame per tick (Director's default tempo, 15 fps) ---

const TICK_MS = 1000 / 15;
let ticker = null;

function stopPlayback() {
  if (ticker) clearTimeout(ticker);
  ticker = null;
}

function startPlayback(movie) {
  stopPlayback();
  let last = performance.now();
  let busy = false;
  const tick = async () => {
    if (busy) return;
    busy = true;
    try {
      const now = performance.now();
      const rc = wasm.play_step(Math.round(now - last));
      last = now;
      if (rc === -1) throw new Error(result());
      if (rc === -2) {
        status(`o Lingo pediu o filme "${result()}" (troca de filme ainda não implementada)`, true);
        stopPlayback();
        return;
      }
      if (rc === 0) {
        // The step read files not fetched yet: fetch them and replay.
        await addFiles(result().split("\n"), "arquivos que o Lingo leu");
        for (let round = 0; ; round++) {
          if (round > 40) throw new Error("a execução continua pedindo arquivos");
          const n = callStr("play", movie);
          if (n === 0) break;
          await addFiles(result().split("\n"), "arquivos que o Lingo leu");
        }
      }
      await drawPlay();
      $("frame").value = wasm.play_frame();
    } catch (e) {
      status(e.message, true);
      stopPlayback();
      return;
    } finally {
      busy = false;
    }
    if (ticker !== null) ticker = setTimeout(tick, TICK_MS);
  };
  ticker = setTimeout(tick, TICK_MS);
}

async function drawPlay() {
  for (let i = 0; i < 5; i++) {
    const drawn = wasm.play_render();
    if (drawn < 0) throw new Error(result());
    const n = wasm.missing();
    if (n === 0) break;
    await addFiles(result().split("\n"), "imagens vinculadas");
  }
  const px = new Uint8ClampedArray(wasm.memory.buffer, wasm.frame_ptr(), width * height * 4);
  $("stage").getContext("2d").putImageData(new ImageData(px.slice(), width, height), 0, 0);
}

function draw(frame) {
  frame = Math.min(Math.max(1, frame | 0), Math.max(frames, 1));
  $("frame").value = $("slider").value = frame;
  const t0 = performance.now();
  const drawn = wasm.render(frame);
  if (drawn < 0) { status(result(), true); return; }
  const px = new Uint8ClampedArray(wasm.memory.buffer, wasm.frame_ptr(), width * height * 4);
  $("stage").getContext("2d").putImageData(new ImageData(px.slice(), width, height), 0, 0);
  const ms = (performance.now() - t0).toFixed(1);
  status(`frame ${frame}/${frames}: ${drawn} sprites, ${ms} ms`);
  history.replaceState(null, "", `#${$("movie").value}/${frame}`);
}

async function main() {
  const { instance } = await WebAssembly.instantiateStreaming(fetch("pkg/web.wasm"),
    { env: { js_glyph, js_line } });
  wasm = instance.exports;

  for (const m of MOVIES) $("movie").append(new Option(m, m));
  const [hashMovie, hashFrame] = location.hash.slice(1).split("/");
  if (MOVIES.includes(hashMovie)) $("movie").value = hashMovie;

  $("movie").onchange = () => openMovie($("movie").value).catch((e) => status(e.message, true));
  $("lingo").onchange = () => openMovie($("movie").value).catch((e) => status(e.message, true));
  loadOwner();
  $("regForm").onsubmit = (ev) => {
    ev.preventDefault();
    applyOwner();
    openMovie($("movie").value).catch((e) => status(e.message, true));
  };
  if ($("regName").value && $("regEmail").value) applyOwner();
  $("frame").onchange = () => draw(+$("frame").value);
  $("slider").oninput = () => draw(+$("slider").value);
  $("prev").onclick = () => draw(+$("frame").value - 1);
  $("next").onclick = () => draw(+$("frame").value + 1);
  addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT") return;
    if (e.key === "ArrowLeft") draw(+$("frame").value - 1);
    if (e.key === "ArrowRight") draw(+$("frame").value + 1);
  });

  await openMovie($("movie").value);
  if (hashFrame) draw(+hashFrame);
}

main().catch((e) => status(e.message, true));
