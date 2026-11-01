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
  const [n, e, c, k] = raw.split("\n");
  $("regName").value = n ?? "";
  $("regEmail").value = e ?? "";
  $("regCep").value = c ?? "";
  $("regCode").value = k ?? "";
}

function applyOwner() {
  const v = [$("regName").value, $("regEmail").value, $("regCep").value, $("regCode").value].join("\n");
  callStr("set_owner", v);
  sessionStorage.setItem(OWNER_KEY, v);
  if ($("regRemember").checked) localStorage.setItem(OWNER_KEY, v);
  else localStorage.removeItem(OWNER_KEY);
}

// --- the files the program writes (prefs, registration, activation) ---
// Kept in this browser like the Windows disk keeps them, next to the owner's
// data (sessionStorage; localStorage too when "lembrar" is checked). The
// program finds its own files again; nothing is reset or altered (ADR 0005).
// Regression: every page load started as a fresh install.

const STATE_KEY = "ilumina-state";
let savedState = "";
let lastSave = 0;

function store(kind) {
  try {
    return kind === "local" ? localStorage : sessionStorage;
  } catch {
    return null;
  }
}

function loadState() {
  let raw = null;
  try {
    raw = store("session")?.getItem(STATE_KEY) ?? store("local")?.getItem(STATE_KEY);
  } catch { /* storage blocked: start fresh */ }
  savedState = raw ?? "";
  callStr("set_state", savedState);
}

/** Keep what the program wrote (call after reading the result buffer). */
function saveState() {
  lastSave = performance.now();
  callStr("state", "");
  const v = result();
  if (v === savedState) return;
  savedState = v;
  try {
    store("session")?.setItem(STATE_KEY, v);
    if ($("regRemember").checked) store("local")?.setItem(STATE_KEY, v);
    else store("local")?.removeItem(STATE_KEY);
  } catch (e) {
    console.warn("ilumina: could not keep the program's files", e);
  }
}

// A session: the movie the startup ran from (replays start there).
let sessionMovie = null;

/** Run (or replay) the session until it ran or a dialog waits. */
async function runPlay() {
  const name = sessionMovie.split("/").pop();
  for (let round = 1; ; round++) {
    if (round > 60) throw new Error("a execução continua pedindo arquivos");
    status(`${name}: executando o Lingo (rodada ${round})…`);
    await new Promise((r) => setTimeout(r)); // let the status paint
    const rc = callStr("play", sessionMovie);
    if (rc === 1) {
      await addFiles(result().split("\n"), "arquivos que o Lingo leu");
      continue;
    }
    // Read the result before `syncMovie`, which uses the same buffer.
    // Regression: the steps log showed the movie path instead of the steps.
    const text = result();
    await syncMovie();
    saveState();
    if (rc === 2) {
      showDialog(text);
      return "dialog";
    }
    return text.split("\n");
  }
}

/** The stage may now show another movie (`go to movie`). */
async function syncMovie() {
  callStr("current_movie", "");
  const movie = result();
  const name = movie.split("/").pop().replace(/\.dxr$/i, "");
  if (MOVIES.includes(name)) $("movie").value = name;
  width = wasm.stage_width();
  height = wasm.stage_height();
  const c = $("stage");
  if (c.width !== width || c.height !== height) {
    c.width = width;
    c.height = height;
  }
}

async function play(movie) {
  const t0 = performance.now();
  sessionMovie = movie;
  hideDialog();
  pendingMouse.length = 0;
  const name = movie.split("/").pop();
  const r = await runPlay();
  await drawPlay();
  if (r === "dialog") {
    status(`${name}: o programa abriu um diálogo`);
    return;
  }
  afterRun(r, t0);
}

function afterRun(steps, t0) {
  $("steps").textContent = steps.join("\n");
  const stop = steps.find((s) => !s.endsWith(": OK"));
  const ms = Math.round(performance.now() - t0);
  const name = sessionMovie.split("/").pop();
  status(stop ? `${name}: parou em ${stop.split(":")[0]} (${ms} ms)` : `${name}: executando (${ms} ms)`, !!stop);
  if (!stop) startPlayback();
}

// --- dialogs (Mui): drawn over the stage; the answer replays the session ---

function hideDialog() {
  $("dialog").hidden = true;
  $("dialog").replaceChildren();
}

function showDialog(text) {
  stopPlayback();
  const [head, ...rows] = text.split("\n");
  const [title, w, h] = head.split("\t");
  const box = $("dialog");
  box.replaceChildren();
  box.hidden = false;
  const win = document.createElement("div");
  win.className = "win";
  win.style.width = `${Math.max(+w, 200)}px`;
  win.style.height = `${Math.max(+h, 80) + 24}px`;
  const bar = document.createElement("div");
  bar.className = "bar";
  bar.textContent = title.trim();
  const x = document.createElement("button");
  x.textContent = "×";
  x.title = "Fechar";
  x.onclick = () => submitDialog(["close"]);
  bar.append(x);
  win.append(bar);
  const body = document.createElement("div");
  body.className = "body";
  win.append(body);
  const fields = [];
  rows.forEach((row, i) => {
    const pos = i + 1;
    const [kind, ix, iy, iw, ih, enabled, value, label] = row.split("\t");
    const unesc = (t) => (t ?? "").replaceAll("\\r", "\n");
    const k = kind.toLowerCase();
    let el = null;
    if (k === "label") {
      // Mui labels show their value (the title stays the default "title").
      el = document.createElement("div");
      el.textContent = unesc(value);
    } else if (k === "edittext" || k === "editTextLarge".toLowerCase()) {
      el = document.createElement("input");
      el.value = unesc(value);
      fields.push({ pos, el, initial: el.value, kind: "text" });
      // The program reacts while the user types (`itemChanged`): it enables
      // "Continuar" once the activation code is in. Send the edit after a
      // pause and redraw the dialog it leaves. Regression: edits only went
      // out with a button click, so the disabled button never came on.
      let timer = null;
      el.oninput = () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
          const lines = changes(fields);
          if (lines.length) submitDialog(lines, pos);
        }, 500);
      };
    } else if (k === "radiobutton" || k === "checkbox") {
      el = document.createElement("label");
      const inp = document.createElement("input");
      inp.type = k === "radiobutton" ? "radio" : "checkbox";
      inp.name = "g";
      inp.checked = value === "1" || value === "true";
      // Choosing an option only changes it; buttons send the choices.
      fields.push({ pos, el: inp, initial: inp.checked, kind: "toggle" });
      el.append(inp, " ", unesc(label));
    } else if (k.endsWith("pushbutton")) {
      el = document.createElement("button");
      el.textContent = unesc(label);
      if (k.startsWith("default")) el.className = "default";
      el.onclick = () => submitDialog(changes(fields).concat([`click\t${pos}`]));
    } else if (k.startsWith("divider")) {
      el = document.createElement("hr");
    }
    if (!el) return;
    if (enabled === "0") el.classList.add("disabled"), el.disabled = true;
    Object.assign(el.style, {
      position: "absolute", left: `${ix}px`, top: `${iy}px`,
      width: +iw > 0 ? `${iw}px` : "auto", minHeight: +ih > 0 ? `${Math.min(+ih, 400)}px` : "auto",
    });
    body.append(el);
  });
  box.append(win);
  const again = fields.find((f) => f.pos === refocus);
  const first = again ?? fields.find((f) => f.kind === "text");
  refocus = 0;
  if (first) {
    first.el.focus();
    if (again) first.el.setSelectionRange(first.el.value.length, first.el.value.length);
  }
}

/** The text field to focus again after a live edit redraws the dialog. */
let refocus = 0;

/** `set` lines for the text fields the user changed. */
function changes(fields) {
  return fields.filter((f) => (f.kind === "toggle" ? f.el.checked : f.el.value) !== f.initial)
    .map((f) => f.kind === "toggle"
      ? `set\t${f.pos}\t${f.el.checked ? 1 : 0}`
      : `set\t${f.pos}\t${f.el.value.replaceAll("\n", "\r")}`);
}

/** Answer the dialog; `typing` = a live edit from field `typing` (the dialog
 * stays up, read-only, until the program redraws it). */
async function submitDialog(lines, typing = 0) {
  if (typing) {
    refocus = typing;
    $("dialog").querySelectorAll("input,button").forEach((e) => (e.disabled = true));
  } else {
    hideDialog();
  }
  try {
    callStr("answer", lines.join("\n"));
    const t0 = performance.now();
    const r = await runPlay();
    await drawPlay();
    if (r !== "dialog") afterRun(r, t0);
  } catch (e) {
    status(e.message, true);
  }
}

// --- playback: one frame per tick (Director's default tempo, 15 fps) ---

const TICK_MS = 1000 / 15;
let ticker = null;
const pendingMouse = [];

function stopPlayback() {
  if (ticker) clearTimeout(ticker);
  ticker = null;
}

/** React to a step/mouse result code; false = playback must stop. */
async function handle(rc) {
  if (rc === -1) {
    const msg = result();
    if (msg === "the program quit") throw new Error("o programa foi encerrado (quit)");
    throw new Error(msg);
  }
  if (rc === 1) {
    // Files not fetched yet: fetch them and replay the session.
    await addFiles(result().split("\n"), "arquivos que o Lingo leu");
    const r = await runPlay();
    return r !== "dialog";
  }
  if (rc === 2) {
    showDialog(result());
    return false;
  }
  if (rc === 3) {
    await syncMovie();
    status(`${$("movie").value}: carregado pelo Lingo`);
  }
  return true;
}

function startPlayback() {
  stopPlayback();
  let last = performance.now();
  let busy = false;
  const tick = async () => {
    if (busy) return;
    busy = true;
    let go = true;
    try {
      while (go && pendingMouse.length) {
        const [kind, x, y] = pendingMouse.shift();
        go = await handle(wasm.play_mouse(kind, x, y));
      }
      if (go) {
        const now = performance.now();
        go = await handle(wasm.play_step(Math.round(now - last)));
        last = now;
      }
      await drawPlay();
      $("frame").value = wasm.play_frame();
      if (performance.now() - lastSave > 3000) saveState();
    } catch (e) {
      status(e.message, true);
      go = false;
    } finally {
      busy = false;
    }
    if (go && ticker !== null) ticker = setTimeout(tick, TICK_MS);
    else stopPlayback();
  };
  ticker = setTimeout(tick, TICK_MS);
}

/** Mouse on the stage canvas → stage coordinates, queued for the next tick. */
function stagePoint(ev) {
  const r = $("stage").getBoundingClientRect();
  return [Math.round((ev.clientX - r.left) * width / r.width),
    Math.round((ev.clientY - r.top) * height / r.height)];
}

function queueMouse(kind, ev) {
  if (!$("lingo").checked || ticker === null) return;
  const [x, y] = stagePoint(ev);
  const last = pendingMouse[pendingMouse.length - 1];
  if (kind === 0 && last && last[0] === 0) last.splice(1, 2, x, y); // coalesce moves
  else pendingMouse.push([kind, x, y]);
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
  $("stage").addEventListener("mousemove", (e) => queueMouse(0, e));
  $("stage").addEventListener("mousedown", (e) => queueMouse(1, e));
  $("stage").addEventListener("mouseup", (e) => queueMouse(2, e));
  loadOwner();
  loadState();
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
