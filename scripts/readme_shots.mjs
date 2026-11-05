// Dev tool: regenerates the screenshots used by README.md and docs/USER_GUIDE.md
// (docs/screenshots/*.png).
//
//   node scripts/readme_shots.mjs [--only ride-dark,phone-ride] [--out docs/screenshots]
//                                 [--exe target/debug/bike-osm-ride.exe] [--port 9333] [--list]
//
// Needs Node 22+ (built-in fetch and WebSocket) and a built ride-app on Windows: the DevTools
// port is a WebView2 feature (`RIDE_DEBUG_PORT`, see crates/ride-app/src/main.rs).
//
// What it does, per scenario: starts the app with that scenario's `RIDE_*` hooks (see
// crates/ride-app/src/hooks.rs) and a throw-away data directory, so no setting, recent route,
// ride or Google key of the person running it can end up in a picture; waits until
// the page is up; pins the page to a fixed size and pixel density through the DevTools protocol
// (the pictures do not depend on the screen they were made on); saves a PNG; and kills the whole
// process tree. The map tiles come from the throw-away data directory's own cache, which the
// first run fills from the OpenStreetMap servers with the app's normal User-Agent (a few dozen
// tiles). Nothing outside `target/readme-shots-data` and the output folder is written.
//
// The route of the pictures is cut from `tests/fixtures/brouter_budapest_dobogoko.gpx` (real
// BRouter output, (c) OpenStreetMap contributors, ODbL) into the data directory at start-up.
//
// Pictures are meant to be regenerated rarely: every new PNG adds to the repository history.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// ───────────────────────────── scenarios ─────────────────────────────

const DESKTOP = { width: 1440, height: 900, dpr: 1 };
const PHONE = { width: 412, height: 860, dpr: 2 };
/** Tall enough for the review page's map, charts and table in one picture. */
const REVIEW = { width: 1440, height: 1250, dpr: 1 };

/** Click the compact layout's tab with this label (phone layout only). */
const tab = label =>
  `(() => { const b = [...document.querySelectorAll('.compact-tab')].find(x => x.textContent.includes(${JSON.stringify(label)})); if (!b) return 'no tab ${label}'; b.click(); return 'ok'; })()`;

/**
 * Click a button of the review page: inside `scope`, the first button with this text, or (with no
 * text) the first one matching `selector`.
 */
const reviewButton = (scope, text, selector = 'button') =>
  `(() => { const b = [...document.querySelectorAll(${JSON.stringify(scope)} + ' ' + ${JSON.stringify(selector)})].find(x => ${text === null ? 'true' : `x.textContent.includes(${JSON.stringify(text)})`}); if (!b) return 'no button ${text ?? selector} in ${scope}'; b.click(); return 'ok'; })()`;

/** Close a panel of the tiled layout (its neighbour takes the space), by its `data-panel` key. */
const closePanel = key =>
  `(() => { const b = document.querySelector('.dock-panel[data-panel="${key}"] .dock-close'); if (!b) return 'no panel ${key}'; b.click(); return 'ok'; })()`;

// RIDE_TIMESCALE fast-forwards the ride clock. Keep it at 20 or below for the pictures of a
// running ride: the engine's 3 s power average needs two samples inside 3 s of ride time, and
// one 100 ms tick of a faster clock is already longer than that (the power hero would be empty).
//
// The route is the last 12.4 km of the repository's real BRouter fixture (Hűvösvölgy - Dobogókő):
// it follows real roads (the synthetic demo route runs across fields and water on the map) and
// it climbs for most of its length, so a fast-forwarded ride lands on the climb in about a minute.
//
// Fields: `settle` seconds to let the app run after the page is up (x20 makes them minutes of
// riding); `waitFor` a selector that must exist before the picture (up to 90 s); `seed` fields
// merged into the throw-away settings.json before the start; `steps` page actions run before the
// picture, each followed by its own pause: `js` (a script), `drag` (a real mouse drag from the
// element a script returns to a point), `scroll` (the scrollable part of the open dialog).
const ROUTE = { RIDE_GPX: '$ROUTE' };
const RIDER = { RIDE_SIM: '190' };
const FAST = { RIDE_TIMESCALE: '20' };
const WORKOUT = { RIDE_WORKOUT: 'c-threshold-2x20' };
const DARK = { RIDE_THEME: 'dark' };
const LEARNED_SWIPE = { ui: { profile_swipe_learned: true } };

/** The parked panel chip with this label (a script that returns the element). */
const chip = label =>
  `[...document.querySelectorAll('.shell-chip')].find(x => x.textContent.includes(${JSON.stringify(label)}))`;

const scenarios = [
  {
    name: 'ride-dark',
    about: 'the ride screen, dark theme: a real route, simulated rider at 190 W, on the climb',
    env: { ...ROUTE, ...RIDER, ...DARK, ...FAST },
    settle: 36,
    steps: [{ js: closePanel('rider'), settle: 2 }],
    view: DESKTOP,
  },
  {
    name: 'ride-light',
    about: 'the same screen in the light theme',
    env: { ...ROUTE, ...RIDER, RIDE_THEME: 'light', ...FAST },
    settle: 36,
    steps: [{ js: closePanel('rider'), settle: 2 }],
    view: DESKTOP,
  },
  {
    name: 'workout',
    about: 'a threshold workout in ERG mode: the workout card, live charts and the block profile',
    env: { ...ROUTE, RIDE_SIM: '194', ...DARK, ...WORKOUT, ...FAST },
    settle: 55,
    steps: [
      { js: closePanel('rider'), settle: 2 },
      { drag: { from: chip('Grafikonok'), x: 180, y: 560 }, settle: 3 },
    ],
    view: DESKTOP,
  },
  {
    name: 'workout-picker',
    about: 'the workout library: filters, the zone-coloured block chart, the block list',
    env: { ...ROUTE, ...DARK, ...WORKOUT, RIDE_DIALOG: 'workouts' },
    settle: 6,
    view: DESKTOP,
  },
  {
    name: 'route-dialog',
    about: 'the route chooser: file, bikerouter link, demo, recent routes (needs a ride scenario first)',
    env: { ...DARK },
    settle: 5,
    view: DESKTOP,
  },
  {
    name: 'devices',
    about: 'the devices dialog with the simulated rider connected',
    env: { ...ROUTE, RIDE_SIM: '200', ...DARK, RIDE_DIALOG: 'devices' },
    settle: 6,
    view: DESKTOP,
  },
  {
    name: 'settings',
    about: 'the settings dialog, light theme',
    env: { ...ROUTE, RIDE_THEME: 'light', RIDE_DIALOG: 'settings' },
    settle: 6,
    view: DESKTOP,
  },
  {
    name: 'summary',
    about: 'the end-of-ride summary: statistics, zone distribution, climbs (the whole route at 60x)',
    env: { ...ROUTE, ...RIDER, ...DARK, RIDE_TIMESCALE: '60' },
    waitFor: '.dialog',
    settle: 8,
    steps: [],
    view: DESKTOP,
  },
  {
    name: 'phone-ride',
    about: 'phone layout, the Tekerés tab, with a workout',
    env: { ...ROUTE, RIDE_SIM: '194', ...DARK, ...WORKOUT, ...FAST },
    seed: LEARNED_SWIPE,
    settle: 55,
    view: PHONE,
  },
  {
    name: 'phone-map',
    about: 'phone layout, the Térkép tab',
    env: { ...ROUTE, ...RIDER, ...DARK, ...FAST },
    seed: LEARNED_SWIPE,
    settle: 33,
    steps: [{ js: tab('Térkép'), settle: 3 }],
    view: PHONE,
  },
  {
    name: 'review',
    about: 'the review page: two rides of one route (one imported, watts estimated), zoomed to the climb',
    env: { ...ROUTE, ...DARK, RIDE_REVIEW: '1' },
    seedRides: true,
    waitFor: '.rv-table',
    settle: 4,
    steps: [
      { js: reviewButton('.rv-section-bar', 'emelkedő'), settle: 3 },
      { js: reviewButton('.rv-ride-actions', null, '[aria-label="A becslés beállításai"]'), settle: 2 },
      { move: `document.querySelector('.rv-plot .rv-hit')`, settle: 1 },
    ],
    view: REVIEW,
  },
  {
    name: 'review-power',
    about: 'the power view of the review page: best powers, the FTP suggestion and the weekly load',
    env: { ...ROUTE, ...DARK, RIDE_REVIEW: '1' },
    seedRides: true,
    waitFor: '.rv-table',
    settle: 4,
    steps: [{ js: reviewButton('.rv-views', 'Teljesítmény'), settle: 3 }],
    view: { width: 1440, height: 1060, dpr: 1 },
  },
  {
    name: 'review-phone',
    about: 'phone layout of the review page, the Grafikon tab',
    env: { ...ROUTE, ...DARK, RIDE_REVIEW: '1' },
    seedRides: true,
    waitFor: '.rv-table',
    settle: 4,
    steps: [{ js: reviewButton('.rv-tabs', 'Grafikon'), settle: 3 }],
    view: PHONE,
  },
];

// ───────────────────────────── plumbing ─────────────────────────────

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function parseArgs(argv) {
  const out = { only: null, out: path.join(root, 'docs', 'screenshots'), port: 9333, list: false };
  out.exe = path.join(root, 'target', 'debug', process.platform === 'win32' ? 'bike-osm-ride.exe' : 'bike-osm-ride');
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--list') out.list = true;
    else if (arg === '--only') out.only = argv[++i].split(',').map(s => s.trim()).filter(Boolean);
    else if (arg === '--out') out.out = path.resolve(argv[++i]);
    else if (arg === '--exe') out.exe = path.resolve(argv[++i]);
    else if (arg === '--port') out.port = Number(argv[++i]);
    else throw new Error(`unknown argument: ${arg}`);
  }
  return out;
}

/** Files and folders of the data directory that make a scenario unclean. `tiles` and `webview` stay. */
const STATE = ['settings.json', 'recent.json', 'routes', 'rides', 'workouts', 'streetview', 'crash.log', 'crash.log.1', 'ble.log', 'ble.log.1'];

function wipe(dataDir, names) {
  for (const name of names) fs.rmSync(path.join(dataDir, name), { recursive: true, force: true });
}

/**
 * Cuts the part of the real BRouter fixture that starts `fromKm` kilometres into it and writes it
 * as a GPX of its own (new name, no BRouter header: its numbers describe the whole fixture).
 */
function prepareRoute(dataDir, fromKm) {
  const source = fs.readFileSync(path.join(root, 'tests', 'fixtures', 'brouter_budapest_dobogoko.gpx'), 'utf8');
  const points = [...source.matchAll(/<trkpt lon="([\d.-]+)" lat="([\d.-]+)"><ele>([\d.-]+)<\/ele><\/trkpt>/g)]
    .map(m => ({ lon: Number(m[1]), lat: Number(m[2]), ele: m[3] }));
  if (points.length < 100) throw new Error('the Dobogókő fixture has changed shape: update prepareRoute()');
  const rad = degrees => (degrees * Math.PI) / 180;
  let distance = 0;
  const kept = [];
  points.forEach((p, i) => {
    if (i) {
      const q = points[i - 1];
      const a = Math.sin(rad(p.lat - q.lat) / 2) ** 2
        + Math.cos(rad(q.lat)) * Math.cos(rad(p.lat)) * Math.sin(rad(p.lon - q.lon) / 2) ** 2;
      distance += 2 * 6371000 * Math.asin(Math.sqrt(a));
    }
    if (distance >= fromKm * 1000) kept.push(p);
  });
  const body = kept.map(p => `   <trkpt lon="${p.lon}" lat="${p.lat}"><ele>${p.ele}</ele></trkpt>`).join('\n');
  const gpx = `<?xml version="1.0" encoding="UTF-8"?>
<gpx xmlns="http://www.topografix.com/GPX/1.1" creator="bike-osm-ride screenshots" version="1.1">
 <trk>
  <name>Dobogókő felé</name>
  <trkseg>
${body}
  </trkseg>
 </trk>
</gpx>
`;
  const file = path.join(dataDir, 'dobogoko-fele.gpx');
  fs.writeFileSync(file, gpx);
  return file;
}

/** Merges `seed` into a settings.json that holds nothing else (`#[serde(default)]` fills the rest). */
function seedSettings(dataDir, seed) {
  fs.writeFileSync(path.join(dataDir, 'settings.json'), JSON.stringify(seed, null, 2));
}

/**
 * Stores rides of the picture route in the throw-away data directory: two made by the ride engine
 * and one imported from a GPX without watts (`crates/ride-route/examples/seed_review.rs`).
 */
function seedRides(dataDir, routeFile) {
  const run = spawnSync('cargo', ['run', '-q', '-p', 'ride-route', '--example', 'seed_review', '--', dataDir, routeFile],
    { cwd: root, stdio: 'inherit' });
  if (run.status !== 0) throw new Error('seeding the rides failed');
}

/** A real left click, or a drag, with trusted mouse events (the tiling dock listens to those). */
async function pointerCenter(cdp, finder) {
  const box = await cdp.evaluate(
    `(() => { const e = ${finder}; if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
  if (!box) throw new Error(`no element for ${finder}`);
  return { x: box[0], y: box[1] };
}

/** Moves the mouse onto the centre of an element, so its hover read-outs show. */
async function moveOnto(cdp, finder) {
  const at = await pointerCenter(cdp, finder);
  for (let i = 0; i < 4; i++) {
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at.x - 20 + i * 10, y: at.y, button: 'none', buttons: 0 });
    await sleep(40);
  }
}

async function dragFromTo(cdp, finder, x, y) {
  const from = await pointerCenter(cdp, finder);
  const mouse = (type, px, py, buttons) => cdp.send('Input.dispatchMouseEvent',
    { type, x: px, y: py, button: 'left', buttons, clickCount: type === 'mouseMoved' ? 0 : 1 });
  await mouse('mouseMoved', from.x, from.y, 0);
  await mouse('mousePressed', from.x, from.y, 1);
  for (let i = 1; i <= 12; i++) {
    await mouse('mouseMoved', from.x + ((x - from.x) * i) / 12, from.y + ((y - from.y) * i) / 12, 1);
    await sleep(40);
  }
  await sleep(250);
  await mouse('mouseReleased', x, y, 0);
}

let child = null;

function killApp() {
  if (!child || child.exitCode !== null) return;
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } else {
    child.kill('SIGKILL');
  }
}
process.on('exit', killApp);
process.on('SIGINT', () => { killApp(); process.exit(130); });

async function findPage(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`the app exited early (code ${child.exitCode}) - see the log`);
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find(t => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch { /* the port is not open yet */ }
    await sleep(400);
  }
  throw new Error('no DevTools page target (is RIDE_DEBUG_PORT honoured? is the build current?)');
}

async function connect(page) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  ws.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    }
  });
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const mine = ++id;
    const timer = setTimeout(() => reject(new Error(`CDP timeout: ${method}`)), 20000);
    pending.set(mine, message => {
      clearTimeout(timer);
      message.error ? reject(new Error(`${method}: ${message.error.message}`)) : resolve(message.result);
    });
    ws.send(JSON.stringify({ id: mine, method, params }));
  });
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(`page script failed: ${result.exceptionDetails.text}`);
    return result.result?.value;
  };
  return { send, evaluate, close: () => ws.close() };
}

async function waitForSelector(cdp, selector, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`the app exited early (code ${child.exitCode})`);
    if (await cdp.evaluate(`!!document.querySelector(${JSON.stringify(selector)})`)) return;
    await sleep(500);
  }
  throw new Error(`"${selector}" never appeared`);
}

/** Width and height of a PNG from its header (no image library needed). */
function pngSize(buffer) {
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function runScenario(scenario, options, dataDir, routeFile) {
  wipe(dataDir, ['settings.json', 'rides', 'crash.log', 'crash.log.1', 'ble.log', 'ble.log.1']);
  if (scenario.seed) seedSettings(dataDir, scenario.seed);
  if (scenario.seedRides) seedRides(dataDir, routeFile);
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('RIDE_')));
  const hooks = Object.fromEntries(Object.entries(scenario.env).map(([key, value]) => [key, value === '$ROUTE' ? routeFile : value]));
  Object.assign(env, hooks, {
    RIDE_DATA_DIR: dataDir,
    RIDE_DEBUG_PORT: String(options.port),
    RIDE_WINDOW: `${scenario.view.width}x${scenario.view.height}`,
  });
  const log = fs.openSync(path.join(dataDir, 'app-output.log'), 'w');
  child = spawn(options.exe, [], { env, cwd: root, stdio: ['ignore', log, log] });
  try {
    const page = await findPage(options.port, 45000);
    const cdp = await connect(page);
    try {
      await waitForSelector(cdp, '.shell', 45000);
      await cdp.send('Emulation.setDeviceMetricsOverride', {
        width: scenario.view.width,
        height: scenario.view.height,
        deviceScaleFactor: scenario.view.dpr,
        mobile: false,
      });
      if (scenario.waitFor) await waitForSelector(cdp, scenario.waitFor, 90000);
      await sleep(scenario.settle * 1000);
      for (const step of scenario.steps ?? []) {
        if (step.drag) {
          await dragFromTo(cdp, step.drag.from, step.drag.x, step.drag.y);
        } else if (step.move) {
          await moveOnto(cdp, step.move);
        } else if (step.scroll !== undefined) {
          const answer = await cdp.evaluate(
            `(() => { const d = document.querySelector('.dialog'); if (!d) return 'no dialog';
              const s = [d, ...d.querySelectorAll('*')].find(e => e.scrollHeight > e.clientHeight + 40 && /(auto|scroll)/.test(getComputedStyle(e).overflowY));
              if (!s) return 'nothing scrolls'; s.scrollTop = ${Number(step.scroll)}; return 'ok'; })()`);
          if (answer !== 'ok') console.log(`    step said: ${answer}`);
        } else {
          const answer = await cdp.evaluate(step.js);
          if (answer !== 'ok') console.log(`    step said: ${answer}`);
        }
        await sleep((step.settle ?? 1) * 1000);
      }
      const shot = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
      const bytes = Buffer.from(shot.data, 'base64');
      const file = path.join(options.out, `${scenario.name}.png`);
      fs.writeFileSync(file, bytes);
      const { width, height } = pngSize(bytes);
      console.log(`  saved ${path.relative(root, file)}  ${width}x${height}  ${(bytes.length / 1024).toFixed(0)} KB`);
    } finally {
      cdp.close();
    }
  } finally {
    killApp();
    await sleep(1500);
    fs.closeSync(log);
  }
}

// ───────────────────────────── main ─────────────────────────────

const options = parseArgs(process.argv.slice(2));
if (options.list) {
  for (const s of scenarios) console.log(`${s.name.padEnd(16)} ${s.about}`);
  process.exit(0);
}
if (!fs.existsSync(options.exe)) {
  console.error(`missing ${options.exe} - build it first: cargo build -p ride-app`);
  process.exit(1);
}
const chosen = options.only ? scenarios.filter(s => options.only.includes(s.name)) : scenarios;
if (!chosen.length) {
  console.error(`no scenario matches ${options.only}; try --list`);
  process.exit(1);
}
const dataDir = path.join(root, 'target', 'readme-shots-data');
fs.mkdirSync(dataDir, { recursive: true });
fs.mkdirSync(options.out, { recursive: true });
wipe(dataDir, STATE);

const routeFile = prepareRoute(dataDir, 17);
let failed = 0;
for (const scenario of chosen) {
  console.log(`${scenario.name}: ${scenario.about}`);
  try {
    await runScenario(scenario, options, dataDir, routeFile);
  } catch (error) {
    failed += 1;
    console.error(`  FAILED: ${error.message}`);
  }
}
process.exit(failed ? 1 : 0);
