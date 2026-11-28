// Golden fixtures for the Rust port, computed by the upstream TypeScript engine itself.
//
//   node crates/bot-avatar/tests/golden/gen.mjs
//
// (Node 22.18+ runs the .ts sources directly; older Nodes need --experimental-strip-types.)
// Writes tests/golden/fixtures/*.json. Every number is rounded to 1e-7 (the Rust tests compare
// within 1e-6); path texts are stored as a 64-bit hash and their length (see `hash`), except a
// sample kept in full, so the fixtures stay a few MB.

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, '..', '..');
const src = join(crate, 'upstream', 'src');
const { avatarFrame, liveFrame, ribbonPointsOf, preparedVolume, avatarExtent } = await import(join(src, 'render.ts'));
const { validateAvatar } = await import(join(src, 'validate.ts'));
const { createAnimator } = await import(join(src, 'animator.ts'));
const { examples } = await import(join(src, 'examples.ts'));
const { computePose } = await import(join(src, 'motion.ts'));
const { analyzeBody } = await import(join(src, 'validate.ts'));
const { faceRoom } = await import(join(src, 'volume.ts'));
const { silhouetteSeams } = await import(join(src, 'seams.ts'));

const out = join(here, 'fixtures');
mkdirSync(out, { recursive: true });

/** Two 32-bit hashes of the UTF-16 units (FNV-1a and djb2-xor) and the length. */
export function hash(s) {
  let a = 2166136261, b = 5381;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    a = Math.imul(a ^ c, 16777619);
    b = Math.imul(b, 33) ^ c;
  }
  return `${(a >>> 0).toString(16).padStart(8, '0')}${(b >>> 0).toString(16).padStart(8, '0')}:${s.length}`;
}

const R = (v) => (typeof v === 'number' ? (Number.isFinite(v) ? Math.round(v * 1e7) / 1e7 : String(v)) : v);
const RA = (a) => a.map(R);
const box = (b) => RA([b.x, b.y, b.w, b.h]);
const f = (n) => { const r = Math.round(n * 100) / 100; return (Object.is(r, -0) ? 0 : r).toString(); };

function markOut(m) {
  if (m.shape === 'pill') return ['pill', ...RA([m.x, m.y, m.width, m.height, m.radius])];
  if (m.shape === 'arc') return ['arc', hash(m.d), R(m.strokeWidth)];
  return ['path', hash(m.d)];
}

/**
 * Consecutive body slices of one stack whose matrices share a, b, c, d become one run (parallel
 * planes differ only in their translation e, f):
 * ['S', stack, [a, b, c, d], paths, parts, js, [e, f] of the first, [e, f] of the last,
 *  [Σe, Σf, Σk·e, Σk·f]] with `paths` the numbers after the avatar's id prefix, `parts` and `js`
 * the part and slice index of each (its key), k counting from 1 within the run. Sums are taken over the unrounded values in order.
 */
function compact(shapes) {
  const out = [];
  let run = null;
  const close = () => {
    if (!run) return;
    const { stack, abcd, ids, parts, js, ef } = run;
    let se = 0, sf = 0, ke = 0, kf = 0;
    ef.forEach(([e, f], k) => { se += e; sf += f; ke += (k + 1) * e; kf += (k + 1) * f; });
    out.push(['S', stack, RA(abcd), ids, parts, js, RA(ef[0]), RA(ef[ef.length - 1]), RA([se, sf, ke, kf])]);
    run = null;
  };
  for (const s of shapes) {
    const m = s[0] === 's' && /^(\d+)([zx])(\d+)$/.exec(s[1]);
    const n = m && /-(\d+)$/.exec(s[2]);
    if (!m || !n) {
      close();
      out.push(s[0] === 's' ? ['s', s[1], s[2], RA(s[3])] : s);
      continue;
    }
    const [part, stack, j] = [Number(m[1]), m[2], Number(m[3])];
    const abcd = s[3].slice(0, 4);
    if (!(run && run.stack === stack && run.abcd.every((v, i) => v === abcd[i]))) {
      close();
      run = { stack, abcd, ids: [], parts: [], js: [], ef: [] };
    }
    run.ids.push(Number(n[1]));
    run.parts.push(part);
    run.js.push(j);
    run.ef.push([s[3][4], s[3][5]]);
  }
  close();
  return out;
}

/** A frame as data: path ids (their texts go to `table`), shapes in order, surface. */
function frameOut(frame, table, full) {
  for (const p of frame.paths) {
    const entry = { h: hash(p.d), e: p.evenOdd, b: box(p.box), r: p.round === true };
    if (full) entry.d = p.d;
    const known = table[p.id];
    if (known && known.h !== entry.h) throw new Error(`path ${p.id} changed its outline`);
    if (!known || (full && !known.d)) table[p.id] = entry;
  }
  return {
    name: frame.name,
    color: frame.color,
    ink: frame.ink,
    root: RA(frame.root),
    // Body paths by the number after the avatar's id prefix; marks' by their whole id.
    paths: frame.paths.map((p) => { const n = /^oba-[0-9a-z]+-(\d+)$/.exec(p.id); return n ? Number(n[1]) : p.id; }),
    shapes: compact(frame.shapes.map((s, i, all) => {
      if (s.kind === 'slice') return ['s', s.key, s.path, s.matrix];
      if (s.kind === 'eye') return ['e', s.key, RA(s.matrix), markOut(s.mark)];
      const pts = ribbonPointsOf(s);
      const d = `M${pts.map((p) => `${f(p[0])} ${f(p[1])}`).join('L')}Z`;
      if (s.d && s.d !== d) throw new Error('ribbon d differs from its points');
      const g = s.gradient;
      // The matrix is written when it differs from the previous ribbon piece's (null: the same).
      const prev = all.slice(0, i).reverse().find((q) => q.kind === 'ribbon');
      const same = prev && prev.matrix.every((v, k) => v === s.matrix[k]);
      return ['r', s.key, same ? null : RA(s.matrix), s.front, s.fill, RA([g.x1, g.y1, g.x2, g.y2]), g.stops, hash(d), pts.length, RA(pts[0])];
    })),
    surface: {
      view: RA([frame.surface.view.cY, frame.surface.view.sY, frame.surface.view.cP, frame.surface.view.sP, frame.surface.view.facing]),
      scale: R(frame.surface.scale),
      eyes: frame.surface.eyes.map((e) => [markOut(e.mark), R(e.dx), R(e.dy)]),
    },
  };
}

function volumeOut(def, depth) {
  const { vol } = preparedVolume(def, depth);
  const b = vol.bounds;
  const ext = avatarExtent(def, depth);
  return {
    depth,
    bounds: RA([b.minX, b.minY, b.maxX, b.maxY]),
    eyePart: vol.eyePart,
    eyeZ: RA(vol.eyeZ),
    eyeScale: R(vol.eyeScale),
    eyePlanes: vol.eyePlanes.map((p) => RA([p.x, p.y, p.z, p.dzdx, p.dzdy])),
    extent: RA([ext.x, ext.y, ext.size]),
    parts: vol.parts.map((p) => ({
      kind: p.kind,
      depth: R(p.depth),
      thickness: R(p.thickness),
      rxy: R(p.rxy),
      rz: R(p.rz),
      axis: p.axis === undefined ? null : R(p.axis),
      center: RA(p.center),
      bounds: RA([p.bounds.minX, p.bounds.minY, p.bounds.maxX, p.bounds.maxY]),
      d: hash(p.d),
      z: p.zSlices.map((s) => [R(s.at), R(s.inset), hash(s.d), s.evenOdd, box(s.box), s.loops.reduce((n, l) => n + l.length, 0)]),
      x: p.xSlices.map((s) => [R(s.at), hash(s.d), box(s.box), s.loops.reduce((n, l) => n + l.length, 0)]),
    })),
  };
}

const T = 1.2;
const color = (id) => ({ controller: '#ff6a1a', mug: '#f4f4f4', ghost: '#f4f4f4', camera: '#7a7a7a', robot: '#1f7cf2' })[id] ?? '#1fbf68';

/** The grid of options: name → RenderOptions, as given to avatarFrame (and parsed back in Rust). */
function cases(id, rich) {
  const c = color(id);
  const list = [];
  const add = (name, o, live) => list.push({ name, o: { color: c, ...o }, live: !!live });
  for (const state of ['idle', 'thinking', 'working', 'waiting', 'blocked', 'done']) {
    for (const mood of ['neutral', 'happy']) {
      if (!rich && mood === 'happy' && state !== 'idle' && state !== 'waiting') continue;
      add(`${state}-${mood}`, { state, mood, time: T, stateAge: T });
    }
  }
  for (const time of rich ? [0, 0.37, 3.5, 7.9, 31.4] : [0.37, 7.9]) add(`idle-t${time}`, { time });
  for (const stateAge of rich ? [2.0, 2.9, 3.4, 10.6] : [2.9]) add(`working-age${stateAge}`, { state: 'working', time: stateAge + 0.5, stateAge });
  for (const stateAge of rich ? [0.05, 0.8, 2.0, 3.7, 5.0, 7.0] : [0.8]) add(`done-age${stateAge}`, { state: 'done', time: stateAge, stateAge });
  add('gaze-a', { time: T, gaze: { x: 0.6, y: -0.4 } });
  add('gaze-b', { state: 'working', time: T, stateAge: 0.5, gaze: { x: -1, y: 0.5, weight: 0.5 } });
  if (rich) add('gaze-happy', { mood: 'happy', time: 2.2, gaze: { x: 0.3, y: 0.9, weight: 0.8 } });
  for (const depth of [0, 14]) add(`depth${depth}`, { time: 0.37, depth });
  if (rich) add('depth2.3', { time: 0.37, depth: 2.3, state: 'working', stateAge: 2.9 });
  add('view-side', { time: T, view: { yaw: 1.2, pitch: 0.1 } });
  add('view-back', { time: T, view: { yaw: -2.6, pitch: -0.3 } });
  if (rich) add('view-top', { time: T, view: { yaw: 0.3, pitch: 1.1 }, depth: 14 });
  add('reduced-working', { state: 'working', time: 2.9, stateAge: 2.9, reducedMotion: true });
  add('reduced-thinking', { state: 'thinking', mood: 'happy', time: 4, reducedMotion: true, gaze: { x: 0.2, y: 0.2 } });
  if (rich) add('reduced-done', { state: 'done', time: 1, stateAge: 1, reducedMotion: true });
  add('blend-legacy', { state: 'working', time: 2.9, stateAge: 0.2, motionBlend: { from: 'happy', t: 0.4 } });
  add('blend-mood', { state: 'waiting', mood: 'happy', time: 3, stateAge: 3, motionBlend: { from: { state: 'waiting', mood: 'neutral', age: 1 }, t: 0.6 } });
  add('blend-nested', {
    state: 'blocked', time: 5, stateAge: 0.1,
    motionBlend: { from: { state: 'thinking', mood: 'happy', age: 2, blend: { from: { state: 'done', age: 0.9, blend: { from: 'idle', t: 0.3 } }, t: 0.5 } }, t: 0.25 },
  });
  if (rich) add('blend-done-working', { state: 'working', time: 3, stateAge: 2.6, motionBlend: { from: { state: 'done', age: 1.5 }, t: 0.5 } });
  add('colors', { time: T, color: '#abc', eyeColor: '#FEDCBA' });
  if (rich) add('colors-bad', { state: 'done', time: 1, stateAge: 1, color: 'red', eyeColor: '#12345' });
  add('motion-thinking', { motion: 'thinking', time: T });
  for (const size of rich ? [16, 28, 56, 120, 400] : [28, 56]) add(`live-idle-${size}`, { time: 0.37, size }, true);
  add('live-working-56', { state: 'working', time: 2.9, stateAge: 2.9, size: 56 }, true);
  add('live-done-28', { state: 'done', time: 1, stateAge: 1, size: 28 }, true);
  add('live-side-56', { time: 1, view: { yaw: 1.3, pitch: 0.2 }, size: 56 }, true);
  add('live-thinking-48', { state: 'thinking', time: 1, size: 48, depth: 14 }, true);
  return list;
}

// ---------------------------------------------------------------------------

const gallery = readdirSync(join(crate, 'gallery')).filter((n) => n.endsWith('.json') && n !== 'colors.json').sort()
  .map((n) => JSON.parse(readFileSync(join(crate, 'gallery', n), 'utf8')));
const solar = readdirSync(join(crate, 'solar')).filter((n) => n.endsWith('.json')).sort()
  .map((n) => JSON.parse(readFileSync(join(crate, 'solar', n), 'utf8')));
const sets = [
  ...examples.map((d) => ['example', d, true]),
  ...gallery.map((d) => ['gallery', d, false]),
  ...solar.map((d) => ['solar', d, true]),
];

let frames = 0;
const index = [];
for (const [source, def, rich] of sets) {
  const validated = validateAvatar(def);
  if (!validated.ok) throw new Error(`${def.id}: ${validated.errors.join('; ')}`);
  const tables = {};
  const depths = [0, 2.3, 6, 14];
  const volumes = depths.map((depth) => volumeOut(def, depth));
  const list = cases(def.id, rich).map((c, i) => {
    const depth = String(c.o.depth ?? 6);
    const table = (tables[depth] ??= {});
    const frame = (c.live ? liveFrame : avatarFrame)(def, c.o);
    frames++;
    const pose = computePose({
      seed: 0, time: c.o.time ?? 0, state: c.o.state, mood: c.o.mood, motion: c.o.motion, stateAge: c.o.stateAge,
      gaze: c.o.gaze, reducedMotion: c.o.reducedMotion === true, view: c.o.view, blend: c.o.motionBlend,
    });
    return {
      ...c,
      frame: frameOut(frame, table, i === 0 && source === 'example'),
      // The pose for seed 0, so motion can be checked apart from the volume.
      pose: [pose.yaw, pose.pitch, pose.roll, pose.lift, pose.sx, pose.sy, pose.gazeX, pose.gazeY, pose.blink, pose.squintL, pose.squintR,
        pose.eyes.arc, pose.eyes.slit, pose.ribbons, pose.cage, pose.sweep].map(R).concat([pose.marks.map((m) => [m.mark.id, R(m.weight)])]),
    };
  });
  const file = `${source}-${def.id}.json`;
  const body = analyzeBody(validated.value.body.path);
  const room = faceRoom(validated.value, body);
  const strict = {
    area: body.area,
    holes: body.holes.map((h) => [hash(h.d), h.area]),
    subpaths: body.subpaths.map((sp) => [hash(sp.d), sp.hole, sp.probe && RA(sp.probe)]),
    faceRoom: [R(room.boost), R(room.clearance), RA([room.gaze.left, room.gaze.right, room.gaze.up, room.gaze.down]), room.ok],
    seams: silhouetteSeams(body.polys).map((sm) => [sm.side, R(sm.at), R(sm.size)]),
  };
  writeFileSync(join(out, file), JSON.stringify({ source, definition: def, normalized: validated.value, strict, volumes, tables, cases: list }));
  index.push(file);
}

// ---------------------------------------------------------------------------
// Validation: the normalized definitions above, and errors for broken ones.

const broken = [
  null, [], 'x', {},
  { version: 2, id: 'Bad Id', name: '  ', body: { path: 'M0 0' }, eyes: { x: 50, y: 50, spacing: 2, scale: 3 }, extra: 1 },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Z', fill: 'red' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1, z: 0 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Z' }, eyes: { x: 50, y: 46, spacing: 15 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Q' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Z' }, eyes: { x: 50, y: 85, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M2 20 H99 V30 H2 Z' }, eyes: { x: 50, y: 25, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 L25 20 L25 25 Z' }, eyes: { x: 22, y: 22, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 A 5 5 0 2 1 30 30 Z' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Z <script>' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H300 V80 H20 Z' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M20 20 H80 V80 H20 Z 5' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'Ok', body: { path: 'M 20 20 h60 v60 h-60 z m10 10 v10 h10 v-10 z' }, eyes: { x: 50, y: 60, spacing: 15, scale: 1 } },
  { version: 1, id: 'ok', name: 'A name that is far too long to be accepted here', body: { path: 'M20 20 H80 V80 H20 Z' }, eyes: { x: 50, y: 46, spacing: 15, scale: 1 } },
];
writeFileSync(join(out, 'validate.json'), JSON.stringify(broken.map((input) => ({ input, result: validateAvatar(input) }))));

// Seams on bodies made to have them (no embedded avatar has any).
const seamBodies = [
  'M20 20 H60 V50 H61 V80 H20 Z',
  'M20 20 H60 V40 H61.5 V60 H60 V80 H20 Z',
  'M20 20 H50 V22 H80 V80 H20 Z',
  'M20 20 H80 V80 H51 V81.2 H20 Z',
  'M20 20 H60 V50 H63 V80 H20 Z M70 30 H72 V35 H70 Z',
  'M20 20 C40 15 60 15 80 20 L79 50 L80.8 50.4 L80 80 H20 Z',
];
writeFileSync(join(out, 'seams.json'), JSON.stringify(seamBodies.map((path) => ({
  path, seams: silhouetteSeams(analyzeBody(path).polys).map((sm) => [sm.side, R(sm.at), R(sm.size)]),
}))));

// ---------------------------------------------------------------------------
// Animator sequences: createAnimator stepped through state, mood and look changes.

function blendOut(b) {
  if (!b) return null;
  const from = typeof b.from === 'string' ? { motion: b.from } : b.from;
  return {
    t: R(b.t),
    from: {
      state: typeof from.state === 'object' ? from.state.id : (from.state ?? null),
      mood: from.mood ?? null, motion: from.motion ?? null, age: from.age === undefined ? null : R(from.age),
      blend: blendOut(from.blend),
    },
  };
}

const scripts = {
  'state-changes': [
    [0, 'idle', 'neutral'], [16, 'idle', 'neutral'], [100, 'working', 'neutral'], [250, 'working', 'happy'], [300, 'waiting', 'happy'],
    [420, 'waiting', 'happy'], [700, 'waiting', 'happy'], [800, 'done', 'neutral'], [900, 'blocked', 'neutral'], [1000, 'idle', 'happy'],
    [1100, 'thinking', 'neutral'], [1150, 'idle', 'neutral'], [1200, 'working', 'neutral'], [2500, 'working', 'neutral'], [4000, 'working', 'neutral'],
  ],
  'mount-done': [[1000, 'done', 'neutral'], [1500, 'done', 'neutral'], [5000, 'done', 'happy'], [5200, 'idle', 'happy'], [9000, 'idle', 'happy']],
  'follow': [
    [0, 'idle', 'neutral', [0.5, -0.2]], [16, 'idle', 'neutral', [0.5, -0.2]], [33, 'idle', 'neutral', [0.5, -0.2]], [200, 'idle', 'neutral', [-0.4, 0.6]],
    [216, 'idle', 'neutral', null], [400, 'idle', 'neutral', null, true], [1200, 'idle', 'neutral', null], [3000, 'idle', 'neutral', null],
  ],
  'reduced': [[0, 'idle', 'neutral', [0.3, 0.3], false, true], [16, 'working', 'happy', null, false, true], [40, 'waiting', 'neutral', [0, 1], false, true]],
};
const animator = {};
const def = examples[0];
for (const [name, steps] of Object.entries(scripts)) {
  const a = createAnimator();
  animator[name] = steps.map(([now, state, mood, target = null, hold = false, reducedMotion = false]) => {
    const s = a.step({ now, state, mood, target: target && { x: target[0], y: target[1] }, hold, reducedMotion });
    const o = { color: '#1fbf68', state, mood, time: now / 1000, stateAge: s.stateAge, gaze: s.gaze, motionBlend: s.motionBlend, reducedMotion };
    const table = {};
    return {
      input: { now, state, mood, target, hold, reducedMotion },
      step: { gaze: s.gaze ? RA([s.gaze.x, s.gaze.y, s.gaze.weight]) : null, motionBlend: blendOut(s.motionBlend), stateAge: R(s.stateAge), moving: s.moving },
      frame: frameOut(avatarFrame(def, o), table, false),
      table,
    };
  });
}
writeFileSync(join(out, 'animator.json'), JSON.stringify({ definition: def.id, sequences: animator }));
writeFileSync(join(out, 'index.json'), JSON.stringify({ commit: 'e10afe4', frames, files: index }));
console.log(`${index.length} avatar files, ${frames} frames`);
