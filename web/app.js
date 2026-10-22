// The browser half of the site. The wasm module owns the content, routing,
// templates, search and the particle simulation; this file only draws the
// points it computes and forwards pointer, keys, scroll and the clock.
// If anything here fails, the pre-rendered HTML keeps working on its own.

const root = document.documentElement;
const view = document.getElementById('view');
const canvas = document.getElementById('field');
const seek = document.getElementById('seek');
const seekQ = document.getElementById('seek-q');
const seekR = document.getElementById('seek-r');
const hint = document.getElementById('hint');
const base = new URL(document.baseURI).pathname;
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const enc = new TextEncoder();
const dec = new TextDecoder();

const SHAPE_SEARCH = 6;
const SHAPE_ROUTE = 255;
const NOT_HERE = 'a, button, input, p, li, h1, h2, h3, pre, code, blockquote, img, table, time, nav, #seek';

let w = null;
let site = {};
let searching = false;
let selected = 0;
let hovered = null;

async function load() {
  const url = new URL('field.wasm' + new URL(import.meta.url).search, import.meta.url);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`field.wasm: ${res.status}`);
  if ((res.headers.get('content-type') || '').includes('application/wasm')) {
    return (await WebAssembly.instantiateStreaming(res, {})).instance.exports;
  }
  return (await WebAssembly.instantiate(await res.arrayBuffer(), {})).instance.exports;
}

const out = () => dec.decode(new Uint8Array(w.memory.buffer, w.out_ptr(), w.out_len()));

// Calls an export that takes one string, then reads its string result.
function call(fn, text, ...rest) {
  const bytes = enc.encode(text);
  const ptr = w.alloc(bytes.length);
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
  w[fn](ptr, bytes.length, ...rest);
  w.dealloc(ptr, bytes.length);
  return out();
}

// ---- routing ---------------------------------------------------------------

function relative(href) {
  const u = new URL(href, document.baseURI);
  return u.pathname.startsWith(base) ? u.pathname.slice(base.length) : null;
}

function show(path, instant = false) {
  const v = JSON.parse(call('route', path, Date.now()));
  document.title = v.title;
  document.querySelector('meta[name="description"]')?.setAttribute('content', v.description);
  document.body.dataset.shape = v.shape;
  hovered = null;
  const swap = () => {
    view.innerHTML = v.html;
    view.classList.remove('leaving');
    if (!instant) scrollTo(0, 0);
    localise();
    clock();
    progress();
  };
  if (instant || reduced.matches) return swap();
  view.classList.add('leaving');
  setTimeout(swap, 160);
}

document.addEventListener('click', (e) => {
  if (!w || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const a = e.target.closest('a[href]');
  if (!a || a.target || a.hasAttribute('download')) return;
  const u = new URL(a.href);
  if (u.origin !== location.origin) return;
  const path = relative(u.href);
  if (path === null || /\.(ics|xml|wasm|js|css|txt)$/.test(path)) return;
  if (u.pathname === location.pathname && u.hash) return;
  e.preventDefault();
  closeSearch();
  if (u.href !== location.href) history.pushState(null, '', u.href);
  show(path);
});

addEventListener('popstate', () => w && show(relative(location.href) ?? ''));

// ---- the field reacts to what you are about to do --------------------------

function hover(el) {
  if (el === hovered || !w) return;
  hovered = el;
  if (searching) return;
  if (el) w.field_hover(+el.dataset.shape, +(el.dataset.group || 0));
  else w.field_hover(SHAPE_ROUTE, 0);
}

view.addEventListener('pointerover', (e) => hover(e.target.closest('[data-shape]')));
view.addEventListener('pointerleave', () => hover(null));
view.addEventListener('focusin', (e) => hover(e.target.closest('[data-shape]')));
view.addEventListener('focusout', () => hover(null));

// ---- search: the whole page is the input -----------------------------------

function openSearch(initial = '') {
  if (!w) return;
  searching = true;
  seek.hidden = false;
  seekQ.value = initial;
  w.field_hover(SHAPE_SEARCH, 0);
  runSearch();
  seekQ.focus();
}

function closeSearch() {
  if (!searching) return;
  searching = false;
  seek.hidden = true;
  seekQ.blur();
  w.field_hover(SHAPE_ROUTE, 0);
}

function runSearch() {
  const r = JSON.parse(call('search', seekQ.value));
  seekR.innerHTML = r.html;
  selected = 0;
}

function select(i) {
  const hits = seekR.querySelectorAll('.hit');
  if (!hits.length) return;
  selected = (i + hits.length) % hits.length;
  hits.forEach((h, j) => h.classList.toggle('on', j === selected));
  hits[selected].scrollIntoView({ block: 'nearest' });
}

seekQ.addEventListener('input', runSearch);
seekQ.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowDown') { e.preventDefault(); select(selected + 1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); select(selected - 1); }
  else if (e.key === 'Enter') { e.preventDefault(); seekR.querySelectorAll('.hit')[selected]?.click(); }
  else if (e.key === 'Escape') { e.preventDefault(); closeSearch(); }
});
seekR.addEventListener('pointermove', (e) => {
  const hit = e.target.closest('.hit');
  if (hit) select(+hit.dataset.i);
});
seekR.addEventListener('click', (e) => { if (e.target.closest('a[target]')) closeSearch(); });
seek.addEventListener('pointerdown', (e) => { if (e.target === seek) closeSearch(); });
hint.addEventListener('click', () => openSearch());

document.addEventListener('keydown', (e) => {
  if (!w || searching || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return;
  if (e.target.closest?.('input, textarea, select, [contenteditable]')) return;
  if (e.key === '/') { e.preventDefault(); openSearch(); }
  else if (e.key.length === 1 && e.key.trim()) { e.preventDefault(); openSearch(e.key); }
});

// ---- time ------------------------------------------------------------------

// Event times are written in the event's own zone; show them in the visitor's.
function localise() {
  const fmt = new Intl.DateTimeFormat(undefined, {
    weekday: 'short', day: 'numeric', month: 'short', year: 'numeric',
    hour: '2-digit', minute: '2-digit', timeZoneName: 'short',
  });
  for (const t of view.querySelectorAll('time[data-local]')) {
    const d = new Date(t.getAttribute('datetime'));
    if (Number.isNaN(+d)) continue;
    t.title = `${t.textContent} where it happens`;
    t.textContent = fmt.format(d);
  }
}

// The site keeps its owner's hours: after their midnight, it slows down.
function clock() {
  const tz = site.timezone || 'UTC';
  let time;
  try {
    time = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', hourCycle: 'h23', timeZone: tz }).format(new Date());
  } catch {
    return;
  }
  const hour = parseInt(time, 10);
  const asleep = hour < 7;
  root.dataset.owner = asleep ? 'asleep' : 'awake';
  w?.field_energy(reduced.matches ? 0 : asleep ? 0.35 : 1);
  const el = view.querySelector('[data-clock]');
  if (!el) return;
  const where = site.city ? `in ${site.city}` : tz === 'UTC' ? 'UTC' : `in ${tz.split('/').pop().replace(/_/g, ' ')}`;
  const mood = asleep ? 'probably asleep, so the field breathes slower' : hour >= 22 ? 'winding down' : 'probably awake';
  el.textContent = `${time} ${where} · ${mood}`;
}

// Reading progress is not a bar: the field closes a ring around the text.
function progress() {
  if (!w) return;
  const max = root.scrollHeight - innerHeight;
  w.field_progress(max > 4 ? scrollY / max : 1);
}
addEventListener('scroll', progress, { passive: true });

// ---- pointer: the field leans away; empty space can be dragged -------------

let drag = null;

addEventListener('pointermove', (e) => {
  if (!w) return;
  w.field_pointer(e.clientX, e.clientY, 1);
  if (drag && e.pointerId === drag.id) {
    w.field_drag(e.clientX - drag.x, e.clientY - drag.y, 1);
    drag.x = e.clientX;
    drag.y = e.clientY;
  }
}, { passive: true });

addEventListener('pointerdown', (e) => {
  if (!w || e.button !== 0 || e.target.closest(NOT_HERE)) return;
  drag = { id: e.pointerId, x: e.clientX, y: e.clientY };
  root.classList.add('dragging');
});

function release(e) {
  if (!w) return;
  if (e.pointerType !== 'mouse') w.field_pointer(0, 0, 0);
  if (drag && e.pointerId === drag.id) {
    drag = null;
    root.classList.remove('dragging');
    w.field_drag(0, 0, 0);
  }
}
addEventListener('pointerup', release);
addEventListener('pointercancel', release);
root.addEventListener('pointerleave', () => w?.field_pointer(0, 0, 0));

// When there is nothing to scroll, scrolling turns the object instead.
addEventListener('wheel', (e) => {
  if (w && !searching && root.scrollHeight <= innerHeight + 2) w.field_spin(e.deltaY + e.deltaX);
}, { passive: true });

// ---- drawing ---------------------------------------------------------------

const VERTEX = `
attribute vec4 a;
uniform mat4 m;
uniform float size;
uniform float dist;
varying float v;
void main() {
  gl_Position = m * vec4(a.xyz, 1.0);
  float d = gl_Position.w / dist;
  gl_PointSize = max(size / d, size * 0.6);
  v = a.w * clamp(1.75 - d, 0.2, 1.0);
}`;

const FRAGMENT = `
precision mediump float;
uniform vec3 ink;
varying float v;
void main() {
  vec2 q = gl_PointCoord * 2.0 - 1.0;
  float r = dot(q, q);
  if (r > 1.0) discard;
  float a = v * (1.0 - smoothstep(0.45, 1.0, r));
  gl_FragColor = vec4(ink * a, a);
}`;

function renderer() {
  const gl = canvas.getContext('webgl', { antialias: false, alpha: true, premultipliedAlpha: true, powerPreference: 'low-power' });
  if (!gl) return null;
  const shader = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  };
  const prog = gl.createProgram();
  gl.attachShader(prog, shader(gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(prog, shader(gl.FRAGMENT_SHADER, FRAGMENT));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
  gl.useProgram(prog);

  const n = w.field_count();
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, n * 16, gl.DYNAMIC_DRAW);
  const loc = gl.getAttribLocation(prog, 'a');
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 4, gl.FLOAT, false, 0, 0);
  const uM = gl.getUniformLocation(prog, 'm');
  const uSize = gl.getUniformLocation(prog, 'size');
  const uDist = gl.getUniformLocation(prog, 'dist');
  const uInk = gl.getUniformLocation(prog, 'ink');
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl.clearColor(0, 0, 0, 0);

  const ink = () => {
    const probe = document.createElement('i');
    probe.style.color = 'var(--ink)';
    document.body.append(probe);
    const [r, g, b] = getComputedStyle(probe).color.match(/\d+(\.\d+)?/g).map(Number);
    probe.remove();
    gl.uniform3f(uInk, r / 255, g / 255, b / 255);
  };
  const resize = () => {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const cw = canvas.clientWidth;
    const ch = canvas.clientHeight;
    canvas.width = Math.round(cw * dpr);
    canvas.height = Math.round(ch * dpr);
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.uniform1f(uSize, 2.1 * dpr);
    w.field_resize(cw, ch);
  };
  ink();
  resize();
  addEventListener('resize', resize);
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', ink);

  let last = performance.now();
  const frame = (t) => {
    const dt = Math.min(0.05, (t - last) / 1000);
    last = t;
    const ptr = w.field_step(dt);
    const mem = w.memory.buffer;
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array(mem, ptr, n * 4));
    gl.uniformMatrix4fv(uM, false, new Float32Array(mem, w.field_mvp(), 16));
    gl.uniform1f(uDist, w.field_distance());
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.POINTS, 0, n);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  return gl;
}

// ---- boot ------------------------------------------------------------------

async function start() {
  try {
    w = await load();
  } catch (err) {
    console.warn('field: staying static', err);
    return;
  }
  const small = Math.min(innerWidth, innerHeight) < 700;
  w.boot(Date.now(), -new Date().getTimezoneOffset(), small ? 3200 : 6400);
  w.site();
  site = JSON.parse(out());
  show(relative(location.href) ?? '', true);
  try {
    if (!renderer()) canvas.remove();
  } catch (err) {
    console.warn('field: no WebGL', err);
    canvas.remove();
  }
  hint.hidden = false;
  if (matchMedia('(hover: none)').matches) hint.textContent = 'search';
  root.classList.add('live');
  setInterval(clock, 30_000);
}

start();
