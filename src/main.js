import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, message } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

// ------------------------------------------------------------------ state

const $ = (s) => document.querySelector(s);
const store = {
  get(k, d) {
    try {
      const v = localStorage.getItem(k);
      return v == null ? d : JSON.parse(v);
    } catch {
      return d;
    }
  },
  set(k, v) {
    try {
      localStorage.setItem(k, JSON.stringify(v));
    } catch {}
  },
};

const DEFAULT_OPTIONS = {
  imageQuality: 85,
  maxWidth: null,
  maxHeight: null,
  stripMetadata: false,
  videoQuality: "medium",
  videoMaxHeight: null,
  audioBitrate: 192,
  pdfDpi: 150,
  gifFps: 12,
  gifWidth: 480,
  parallel: 2,
};

const state = {
  items: [], // queue rows
  engines: [],
  options: { ...DEFAULT_OPTIONS, ...store.get("options", {}) },
  outputDir: store.get("outputDir", null),
  lastFormat: store.get("lastFormat", {}), // category -> format
  running: 0,
  stopping: false,
};

let nextId = 1;

const CATEGORY_ICON = {
  image: "🖼",
  video: "🎞",
  audio: "🎵",
  document: "📄",
  spreadsheet: "📊",
  presentation: "📽",
  markup: "📝",
  ebook: "📚",
  pdf: "📕",
  archive: "🗜",
};

const PREFERRED = {
  image: ["jpg", "png"],
  video: ["mp4", "webm"],
  audio: ["mp3", "m4a"],
  document: ["pdf", "docx"],
  spreadsheet: ["xlsx", "pdf"],
  presentation: ["pdf", "pptx"],
  markup: ["docx", "html"],
  ebook: ["epub", "azw3"],
  pdf: ["png", "pdf-compressed"],
  archive: ["folder", "zip"],
};

// ---------------------------------------------------------------- helpers

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

function fmtSize(n) {
  if (!n) return "";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n < 10 && i ? 1 : 0)} ${u[i]}`;
}

function baseName(p) {
  return String(p).split(/[\\/]/).pop();
}

function engineName(id) {
  return state.engines.find((e) => e.id === id)?.name ?? id;
}

let toastTimer;
function toast(msg) {
  const t = $("#toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), 3500);
}

function pickDefaultFormat(info) {
  const avail = info.outputs.filter((o) => o.available).map((o) => o.format);
  const remembered = state.lastFormat[info.category];
  if (remembered && avail.includes(remembered)) return remembered;
  for (const f of PREFERRED[info.category] ?? []) if (avail.includes(f)) return f;
  return avail[0] ?? info.outputs[0]?.format ?? null;
}

// ------------------------------------------------------------------ files

async function addPaths(paths) {
  if (!paths?.length) return;
  let infos;
  try {
    infos = await invoke("inspect", { paths });
  } catch (e) {
    toast(String(e));
    return;
  }
  const existing = new Set(state.items.map((i) => i.path));
  let added = 0,
    unknown = 0;
  for (const info of infos) {
    if (existing.has(info.path)) continue;
    if (!info.category) {
      unknown++;
      continue;
    }
    state.items.push({
      ...info,
      id: `job-${nextId++}`,
      format: pickDefaultFormat(info),
      status: "ready",
      progress: 0,
      result: null,
      error: null,
    });
    added++;
  }
  if (unknown) toast(`${unknown} file${unknown > 1 ? "s" : ""} skipped: unsupported type`);
  if (added) render();
}

/** After an engine is installed or located, refresh which outputs are available. */
async function refreshAvailability() {
  const idle = state.items.filter((i) => i.status !== "running");
  if (!idle.length) return;
  try {
    const infos = await invoke("inspect", { paths: idle.map((i) => i.path) });
    const byPath = new Map(infos.map((i) => [i.path, i]));
    for (const item of idle) {
      const info = byPath.get(item.path);
      if (!info) continue;
      item.outputs = info.outputs;
      const cur = item.outputs.find((o) => o.format === item.format);
      if (!cur || (!cur.available && item.status === "ready")) item.format = pickDefaultFormat(info);
    }
    render();
  } catch {}
}

// ----------------------------------------------------------------- render

function render() {
  const hasItems = state.items.length > 0;
  $("#queue-wrap").hidden = !hasItems;
  $("#dropzone").classList.toggle("compact", hasItems);
  renderQueue();
  renderBulk();
  renderFooter();
}

function statusHtml(item) {
  const out = item.outputs.find((o) => o.format === item.format);
  switch (item.status) {
    case "running":
      return `<div class="bar"><div class="fill" style="width:${Math.round(item.progress * 100)}%"></div></div>`;
    case "queued":
      return `<span class="muted">Waiting…</span>`;
    case "done": {
      const first = item.result?.outputs?.[0];
      return `<span class="ok">✓ Done</span> ${first ? `<button class="link" data-act="reveal" title="${esc(first)}">Show</button>` : ""}`;
    }
    case "cancelled":
      return `<span class="muted">Cancelled</span>`;
    case "error":
      return `<button class="link err" data-act="details" title="Show details">Failed — details</button>`;
    default:
      if (out && !out.available) {
        if (out.missing?.length)
          return `<span class="warn">Needs ${out.missing.map(engineName).join(" + ")}</span> <button class="link" data-act="engines">Set up</button>`;
        return `<span class="warn" title="${esc(out.note ?? "")}">Not available</span>`;
      }
      return "";
  }
}

function rowHtml(item) {
  const busy = item.status === "running" || item.status === "queued";
  const options = item.outputs
    .map((o) => {
      const why = o.available ? "" : o.missing?.length ? ` (needs ${o.missing.map(engineName).join(" + ")})` : " (unavailable)";
      return `<option value="${o.format}" ${o.format === item.format ? "selected" : ""} ${o.available ? "" : "disabled"}>${esc(o.label + why)}</option>`;
    })
    .join("");
  return `
    <li class="item ${item.status}" data-id="${item.id}">
      <span class="icon" aria-hidden="true">${CATEGORY_ICON[item.category] ?? "📁"}</span>
      <div class="meta">
        <div class="name" title="${esc(item.path)}">${esc(item.name)}</div>
        <div class="sub muted">${esc(item.categoryLabel ?? "")} · ${fmtSize(item.size)}</div>
      </div>
      <span class="arrow" aria-hidden="true">→</span>
      <select class="fmt" data-act="format" ${busy ? "disabled" : ""} aria-label="Output format for ${esc(item.name)}">${options}</select>
      <div class="status">${statusHtml(item)}</div>
      <button class="icon-btn" data-act="${busy ? "cancel" : "remove"}" title="${busy ? "Cancel" : "Remove"}" aria-label="${busy ? "Cancel" : "Remove"}">✕</button>
    </li>`;
}

function renderQueue() {
  $("#queue").innerHTML = state.items.map(rowHtml).join("");
}

function updateRow(item) {
  const li = document.querySelector(`li[data-id="${item.id}"]`);
  if (!li) return render();
  li.outerHTML = rowHtml(item);
  renderFooter();
}

function renderBulk() {
  const seen = new Map();
  for (const it of state.items) for (const o of it.outputs) if (o.available && !seen.has(o.format)) seen.set(o.format, o.label);
  const sel = $("#bulk-format");
  sel.innerHTML =
    `<option value="">Choose…</option>` +
    [...seen].map(([f, l]) => `<option value="${f}">${esc(l.replace(/ \(.*\)$/, ""))}</option>`).join("");
}

function pending() {
  return state.items.filter((i) => {
    if (!["ready", "error", "cancelled"].includes(i.status)) return false;
    return i.outputs.find((o) => o.format === i.format)?.available;
  });
}

function renderFooter() {
  const btn = $("#convert-btn");
  const active = state.items.some((i) => i.status === "running" || i.status === "queued");
  if (active) {
    btn.textContent = "Stop";
    btn.classList.add("danger");
    btn.disabled = false;
  } else {
    const n = pending().length;
    btn.classList.remove("danger");
    btn.textContent = n ? `Convert ${n} file${n > 1 ? "s" : ""}` : "Convert";
    btn.disabled = n === 0;
  }
  const od = $("#outdir-btn");
  od.textContent = state.outputDir ? baseName(state.outputDir) : "Same folder as original";
  od.title = state.outputDir ?? "Choose output folder";
  $("#outdir-reset").hidden = !state.outputDir;
}

// -------------------------------------------------------------- converting

function jobOptions() {
  const o = state.options;
  const num = (v) => (v === "" || v == null || isNaN(+v) ? null : +v);
  return {
    outputDir: state.outputDir,
    imageQuality: +o.imageQuality,
    maxWidth: num(o.maxWidth),
    maxHeight: num(o.maxHeight),
    stripMetadata: !!o.stripMetadata,
    videoQuality: o.videoQuality,
    videoMaxHeight: num(o.videoMaxHeight),
    audioBitrate: +o.audioBitrate,
    pdfDpi: +o.pdfDpi,
    gifFps: +o.gifFps,
    gifWidth: +o.gifWidth,
  };
}

function startAll() {
  state.stopping = false;
  for (const i of pending()) {
    i.status = "queued";
    i.progress = 0;
    i.error = null;
  }
  render();
  pump();
}

function pump() {
  const max = Math.max(1, +state.options.parallel || 2);
  while (!state.stopping && state.running < max) {
    const next = state.items.find((i) => i.status === "queued");
    if (!next) break;
    runItem(next);
  }
  if (state.running === 0 && !state.items.some((i) => i.status === "queued")) {
    const done = state.items.filter((i) => i.status === "done").length;
    const failed = state.items.filter((i) => i.status === "error").length;
    if (done + failed > 1) toast(`${done} converted${failed ? `, ${failed} failed` : ""}`);
  }
}

async function runItem(item) {
  state.running++;
  item.status = "running";
  item.progress = 0;
  updateRow(item);
  try {
    item.result = await invoke("convert", {
      id: item.id,
      job: { input: item.path, format: item.format, options: jobOptions() },
    });
    item.status = "done";
  } catch (e) {
    const msg = String(e);
    item.status = msg === "Cancelled" ? "cancelled" : "error";
    item.error = msg;
  }
  state.running--;
  updateRow(item);
  pump();
}

function stopAll() {
  state.stopping = true;
  for (const i of state.items) if (i.status === "queued") i.status = "ready";
  invoke("cancel_all");
  render();
}

listen("job-progress", ({ payload }) => {
  const item = state.items.find((i) => i.id === payload.id);
  if (!item || item.status !== "running") return;
  item.progress = payload.progress;
  const fill = document.querySelector(`li[data-id="${item.id}"] .fill`);
  if (fill) fill.style.width = `${Math.round(payload.progress * 100)}%`;
});

// ---------------------------------------------------------------- engines

const SOURCE_LABEL = { bundled: "Built in", downloaded: "Downloaded", system: "Installed", custom: "Custom" };

async function loadEngines() {
  try {
    state.engines = await invoke("list_engines");
  } catch (e) {
    toast(String(e));
    return;
  }
  renderEngines();
}

function renderEngines() {
  const missing = state.engines.filter((e) => !e.path).length;
  const badge = $("#engine-badge");
  badge.hidden = missing === 0;
  badge.textContent = missing;
  $("#engines").innerHTML = state.engines
    .map((e) => {
      const installed = !!e.path;
      const chip = installed
        ? `<span class="chip ok">${SOURCE_LABEL[e.source] ?? "Ready"}</span>`
        : `<span class="chip warn">Not installed</span>`;
      const primary = installed
        ? ""
        : e.delivery === "ondemand"
          ? `<button class="btn small primary" data-eng="install" data-id="${e.id}">Download</button>`
          : `<button class="btn small primary" data-eng="get" data-id="${e.id}">Get ${esc(e.name)}</button>`;
      return `
      <article class="engine" data-id="${e.id}">
        <header><h3>${esc(e.name)}</h3>${chip}</header>
        <p>${esc(e.purpose)}</p>
        ${installed ? `<p class="path muted" title="${esc(e.path)}">${esc(e.version || e.path)}</p>` : ""}
        <div class="engine-progress" hidden><div class="bar"><div class="fill"></div></div><span class="muted stage"></span></div>
        <div class="engine-actions">
          ${primary}
          <button class="btn small ghost" data-eng="locate" data-id="${e.id}">Locate…</button>
          ${e.source === "custom" ? `<button class="link" data-eng="reset" data-id="${e.id}">Use default</button>` : ""}
          <span class="spacer"></span>
          <span class="muted tiny">${esc(e.license)}</span>
        </div>
      </article>`;
    })
    .join("");
}

listen("engine-progress", ({ payload }) => {
  const card = document.querySelector(`.engine[data-id="${payload.id}"]`);
  if (!card) return;
  const box = card.querySelector(".engine-progress");
  box.hidden = false;
  box.querySelector(".fill").style.width = `${Math.round(payload.progress * 100)}%`;
  box.querySelector(".stage").textContent = payload.stage;
});

$("#engines").addEventListener("click", async (ev) => {
  const btn = ev.target.closest("[data-eng]");
  if (!btn) return;
  const id = btn.dataset.id;
  const eng = state.engines.find((e) => e.id === id);
  const act = btn.dataset.eng;
  if (act === "get") return openUrl(eng.downloadPage);
  if (act === "install") {
    btn.disabled = true;
    try {
      await invoke("install_engine", { id });
      toast(`${eng.name} is ready`);
    } catch (e) {
      toast(String(e));
    }
  } else if (act === "locate") {
    const picked = await open({ multiple: false, directory: false, title: `Locate the ${eng.name} program` });
    if (!picked) return;
    try {
      await invoke("set_engine_path", { id, path: picked });
    } catch (e) {
      return toast(String(e));
    }
  } else if (act === "reset") {
    await invoke("set_engine_path", { id, path: null });
  }
  await loadEngines();
  await refreshAvailability();
});

// ----------------------------------------------------------------- events

function showView(name) {
  for (const t of document.querySelectorAll(".tab")) {
    const on = t.dataset.view === name;
    t.classList.toggle("active", on);
    t.setAttribute("aria-selected", on);
  }
  $("#view-convert").hidden = name !== "convert";
  $("#view-engines").hidden = name !== "engines";
  if (name === "engines") loadEngines();
}

for (const t of document.querySelectorAll(".tab")) t.addEventListener("click", () => showView(t.dataset.view));

async function pickFiles() {
  const picked = await open({ multiple: true, directory: false, title: "Add files to convert" });
  if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
}
async function pickFolder() {
  const picked = await open({ multiple: false, directory: true, title: "Add a folder" });
  if (picked) addPaths([picked]);
}
$("#add-files").addEventListener("click", pickFiles);
$("#add-more").addEventListener("click", pickFiles);
$("#add-folder").addEventListener("click", pickFolder);

$("#queue").addEventListener("change", (ev) => {
  if (ev.target.dataset.act !== "format") return;
  const item = state.items.find((i) => i.id === ev.target.closest("li").dataset.id);
  item.format = ev.target.value;
  if (item.status !== "running") item.status = "ready";
  state.lastFormat[item.category] = item.format;
  store.set("lastFormat", state.lastFormat);
  updateRow(item);
});

$("#queue").addEventListener("click", async (ev) => {
  const btn = ev.target.closest("[data-act]");
  if (!btn || btn.tagName === "SELECT") return;
  const item = state.items.find((i) => i.id === btn.closest("li").dataset.id);
  switch (btn.dataset.act) {
    case "remove":
      state.items = state.items.filter((i) => i !== item);
      render();
      break;
    case "cancel":
      if (item.status === "queued") {
        item.status = "ready";
        updateRow(item);
      } else invoke("cancel", { id: item.id });
      break;
    case "reveal":
      invoke("reveal", { path: item.result.outputs[0] }).catch((e) => toast(String(e)));
      break;
    case "engines":
      showView("engines");
      break;
    case "details":
      message(item.error, { title: `Couldn't convert ${item.name}`, kind: "error" });
      break;
  }
});

$("#bulk-format").addEventListener("change", (ev) => {
  const f = ev.target.value;
  if (!f) return;
  let n = 0;
  for (const i of state.items) {
    if (i.status === "running" || i.status === "queued") continue;
    if (i.outputs.find((o) => o.format === f && o.available)) {
      i.format = f;
      i.status = "ready";
      n++;
    }
  }
  ev.target.value = "";
  render();
  toast(`Set ${n} file${n === 1 ? "" : "s"} to ${f.toUpperCase()}`);
});

$("#clear-done").addEventListener("click", () => {
  state.items = state.items.filter((i) => i.status !== "done");
  render();
});
$("#clear-all").addEventListener("click", () => {
  if (state.running) return toast("Stop the running conversions first");
  state.items = [];
  render();
});

$("#convert-btn").addEventListener("click", () => {
  const active = state.items.some((i) => i.status === "running" || i.status === "queued");
  active ? stopAll() : startAll();
});

$("#outdir-btn").addEventListener("click", async () => {
  const picked = await open({ directory: true, multiple: false, title: "Save converted files to" });
  if (!picked) return;
  state.outputDir = picked;
  store.set("outputDir", picked);
  renderFooter();
});
$("#outdir-reset").addEventListener("click", () => {
  state.outputDir = null;
  store.set("outputDir", null);
  renderFooter();
});

// --------------------------------------------------------------- options

const form = $("#settings form");

function fillForm() {
  for (const el of form.elements) {
    if (!el.name) continue;
    const v = state.options[el.name];
    if (el.type === "checkbox") el.checked = !!v;
    else el.value = v ?? "";
  }
  for (const o of form.querySelectorAll("output")) o.textContent = state.options[o.dataset.for];
}

function readForm() {
  for (const el of form.elements) {
    if (!el.name) continue;
    state.options[el.name] = el.type === "checkbox" ? el.checked : el.value === "" ? null : el.value;
  }
  store.set("options", state.options);
}

form.addEventListener("input", (ev) => {
  readForm();
  const out = form.querySelector(`output[data-for="${ev.target.name}"]`);
  if (out) out.textContent = ev.target.value;
});
$("#settings-reset").addEventListener("click", (ev) => {
  ev.preventDefault();
  state.options = { ...DEFAULT_OPTIONS };
  store.set("options", state.options);
  fillForm();
});
$("#settings-btn").addEventListener("click", () => {
  fillForm();
  $("#settings").showModal();
});

// -------------------------------------------------------------- drag & drop

getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  const overlay = $("#drag-overlay");
  if (p.type === "enter" || p.type === "over") {
    overlay.hidden = false;
  } else if (p.type === "drop") {
    overlay.hidden = true;
    showView("convert");
    addPaths(p.paths);
  } else {
    overlay.hidden = true;
  }
});

// ---------------------------------------------------------------- branding

for (const el of document.querySelectorAll("[data-url]")) {
  el.addEventListener("click", () => openUrl(el.dataset.url).catch((e) => toast(String(e))));
}

// ------------------------------------------------------------------- boot

loadEngines();
render();
