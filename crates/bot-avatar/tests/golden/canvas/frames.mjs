// Painter cross-check, step 1: the cases and the TS frames for them.
//
//   node crates/bot-avatar/tests/golden/canvas/frames.mjs /tmp/oba-browser
//
// Writes <dir>/cases.json (read by the Rust `canvas_compare` test, which paints the same cases
// with tiny-skia into <dir>/rust/*.rgba) and <dir>/frames.json (drawn by index.html in a
// browser with the canvas code of dom/paint.ts). See README.md in this folder.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, '..', '..', '..');
const { avatarFrame, ribbonPointsOf } = await import(join(crate, 'upstream', 'src', 'render.ts'));
const { examples } = await import(join(crate, 'upstream', 'src', 'examples.ts'));
const dir = process.argv[2] ?? '/tmp/oba-browser';
mkdirSync(join(dir, 'rust'), { recursive: true });

const def = (source, id) => source === 'example'
  ? examples.find((e) => e.id === id)
  : JSON.parse(readFileSync(join(crate, source, `${id}.json`), 'utf8'));
const cases = [
  ['controller-idle', 'example', 'controller', { color: '#ff6a1a', time: 0.37 }],
  ['mug-happy', 'example', 'mug', { color: '#f8a01c', mood: 'happy', time: 1 }],
  ['ghost-waiting', 'solar', 'ghost', { color: '#8b5cf6', state: 'waiting', time: 2 }],
  ['robot-working', 'solar', 'robot', { color: '#1f7cf2', state: 'working', time: 2.6, stateAge: 2.6 }],
  ['cat-thinking', 'solar', 'cat', { color: '#ec4899', state: 'thinking', time: 1 }],
  ['barbell-side', 'example', 'barbell', { color: '#f2384b', time: 1, view: { yaw: 1.2, pitch: 0.1 } }],
  ['skull-done', 'solar', 'skull', { color: '#f4f4f4', state: 'done', time: 1, stateAge: 1 }],
  ['invader-flat', 'solar', 'invader', { color: '#14b3a5', time: 0.5, depth: 0 }],
  ['frog-blink-blend', 'solar', 'frog', { color: '#1fbf68', state: 'waiting', mood: 'happy', time: 3, motionBlend: { from: { state: 'idle' }, t: 0.5 } }],
  ['crab-bang', 'solar', 'crab', { color: '#f2384b', state: 'blocked', time: 0.6 }],
];
const SIZE = 160;
const frames = cases.map(([name, source, id, o]) => {
  const f = avatarFrame(def(source, id), o);
  return {
    name, size: SIZE, color: f.color, ink: f.ink, root: f.root,
    paths: f.paths.map((p) => ({ id: p.id, d: p.d, evenOdd: p.evenOdd, box: p.box })),
    shapes: f.shapes.map((s) => (s.kind === 'ribbon' ? { ...s, points: ribbonPointsOf(s) } : s)),
  };
});
writeFileSync(join(dir, 'cases.json'), JSON.stringify(cases.map(([name, source, id, o]) => ({ name, source, id, o, size: SIZE }))));
writeFileSync(join(dir, 'frames.json'), JSON.stringify(frames));
copyFileSync(join(here, 'index.html'), join(dir, 'index.html'));
console.log(`${cases.length} cases in ${dir}`);
