#!/usr/bin/env node
// Layout QA: opens the app in one language and reports elements whose text does not fit.
//
//   node scripts/i18n/overflow.mjs --lang pt [--window 412x860] [--port 9444]
//                                   [--exe target/debug/bike-osm-ride.exe] [--json out.json]
//
// A translation may be longer than the source. This script starts the app with a throw-away data
// directory and the repository's BRouter fixture, walks the visible screens (the ride screen, every
// top-bar dialog, and the phone tabs) and evaluates, on each screen, every visible element whose
// `scrollWidth` is larger than its `clientWidth`. Elements that scroll on purpose (`overflow-x:
// auto/scroll`) are marked as such and do not fail the run; a clipped label (`overflow: hidden`) or
// a spilling child does. It is a layout smell detector, not a pixel-perfect test: a clean run means
// "nothing overflows in these screens at this size", not "the translation is beautiful".
//
// The app is killed afterwards; nothing outside `target/i18n-overflow/` is written.

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const args = process.argv.slice(2);
const value = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const lang = value('--lang', 'hu');
const windowSize = value('--window', '412x860');
const port = Number(value('--port', '9444'));
const exe = path.resolve(value('--exe', path.join(root, 'target', 'debug', 'bike-osm-ride.exe')));
const jsonOut = value('--json', null);
const dataDir = path.join(root, 'target', 'i18n-overflow');
const route = path.join(root, 'tests', 'fixtures', 'brouter_budapest_dobogoko.gpx');

if (!fs.existsSync(exe)) {
  console.error(`missing ${exe}: build it first (cargo build -p ride-app)`);
  process.exit(2);
}
if (!fs.existsSync(route)) {
  console.error(`missing fixture ${route}`);
  process.exit(2);
}
fs.rmSync(dataDir, { recursive: true, force: true });
fs.mkdirSync(dataDir, { recursive: true });

const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('RIDE_')));
Object.assign(env, {
  RIDE_LANG: lang,
  RIDE_DATA_DIR: dataDir,
  RIDE_DEBUG_PORT: String(port),
  RIDE_WINDOW: windowSize,
  RIDE_GPX: route,
  RIDE_SIM: '190',
  RIDE_TIMESCALE: '10',
});

const log = fs.openSync(path.join(dataDir, 'app.log'), 'w');
const child = spawn(exe, [], { cwd: root, env, stdio: ['ignore', log, log] });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const kill = () => {
  try {
    child.kill();
  } catch {
    /* already gone */
  }
};
process.on('exit', kill);
process.on('SIGINT', () => {
  kill();
  process.exit(130);
});

async function findPage(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`the app exited early (code ${child.exitCode})`);
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((target) => target.type === 'page' && target.webSocketDebuggerUrl);
      if (page) return page;
    } catch {
      /* not up yet */
    }
    await sleep(400);
  }
  throw new Error('no DevTools page target');
}

async function connect(page) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  ws.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const mine = ++id;
      const timer = setTimeout(() => reject(new Error(`CDP timeout: ${method}`)), 20000);
      pending.set(mine, (message) => {
        clearTimeout(timer);
        message.error ? reject(new Error(`${method}: ${message.error.message}`)) : resolve(message.result);
      });
      ws.send(JSON.stringify({ id: mine, method, params }));
    });
  const evaluate = async (expression) => {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(`page script failed: ${result.exceptionDetails.text}`);
    return result.result?.value;
  };
  return { send, evaluate, close: () => ws.close() };
}

const SCAN = `(() => {
  const out = [];
  const label = (el) => (el.getAttribute('aria-label') || el.innerText || el.textContent || '').trim().replace(/\\s+/g, ' ').slice(0, 80);
  for (const el of document.querySelectorAll('body *')) {
    const rect = el.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) continue;
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0) continue;
    const dx = el.scrollWidth - el.clientWidth;
    const dy = el.scrollHeight - el.clientHeight;
    if (dx <= 1 && dy <= 1) continue;
    // A container that scrolls on purpose is fine; a clipped or spilling label is not.
    const scrolls = /(auto|scroll)/.test(style.overflowX) || /(auto|scroll)/.test(style.overflowY);
    if (scrolls) continue;
    const text = label(el);
    if (!text) continue;
    out.push({
      tag: el.tagName.toLowerCase(),
      cls: (el.className || '').toString().slice(0, 60),
      text,
      dx,
      dy,
      w: Math.round(rect.width),
      h: Math.round(rect.height),
      overflow: style.overflowX + '/' + style.overflowY,
    });
  }
  return out;
})()`;

const results = [];
async function scan(cdp, screen) {
  await sleep(450);
  const found = (await cdp.evaluate(SCAN)) ?? [];
  results.push({ screen, found });
  const mark = found.length ? '!!' : 'ok';
  console.log(`${mark} ${screen}: ${found.length} overflowing`);
  for (const item of found.slice(0, 12)) {
    console.log(`   ${item.tag}.${item.cls} dx=${item.dx} dy=${item.dy} ${item.w}x${item.h} [${item.overflow}] ${item.text}`);
  }
}

async function pressEscape(cdp) {
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await sleep(350);
}

try {
  const page = await findPage(60000);
  const cdp = await connect(page);
  // The page is up when the shell exists; the first scan waits for it.
  for (let i = 0; i < 120; i++) {
    if (await cdp.evaluate('!!document.querySelector(".shell")')) break;
    await sleep(500);
  }
  await sleep(1500);
  await scan(cdp, 'ride');

  const buttons = (await cdp.evaluate(
    `[...document.querySelectorAll('.shell-tb')].map((b, i) => ({ i, label: (b.getAttribute('aria-label') || b.innerText || '').trim().replace(/\\s+/g, ' ') }))`,
  )) ?? [];
  for (const button of buttons) {
    if (!button.label) continue;
    await cdp.evaluate(`document.querySelectorAll('.shell-tb')[${button.i}]?.click()`);
    await scan(cdp, `dialog: ${button.label}`);
    await pressEscape(cdp);
  }

  // Phone layout: every compact tab.
  const tabs = (await cdp.evaluate(
    `[...document.querySelectorAll('.compact-tab')].map((b, i) => ({ i, label: (b.innerText || '').trim() }))`,
  )) ?? [];
  for (const tab of tabs) {
    if (!tab.label) continue;
    await cdp.evaluate(`document.querySelectorAll('.compact-tab')[${tab.i}]?.click()`);
    await scan(cdp, `tab: ${tab.label}`);
  }
  cdp.close();
} catch (error) {
  console.error(`FAILED: ${error.message}`);
  process.exitCode = 1;
} finally {
  kill();
  await sleep(1000);
  fs.closeSync(log);
}

const offending = results.flatMap((screen) => screen.found.map((item) => ({ screen: screen.screen, ...item })));
console.log(`\n${results.length} screens, ${offending.length} overflow(s), language ${lang}, window ${windowSize}`);
if (jsonOut) {
  fs.writeFileSync(path.resolve(jsonOut), `${JSON.stringify({ lang, window: windowSize, results }, null, 2)}\n`, 'utf8');
  console.log(`written ${jsonOut}`);
}
process.exit(offending.length ? 1 : process.exitCode ?? 0);
