// Windows/WebView2 interaction regressions. Uses an isolated data directory and no public APIs.
// node scripts/ui_regressions.mjs --exe <built bike-osm-ride.exe> --out <scratch directory>
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? fallback : process.argv[i + 1];
};
const exe = path.resolve(arg('--exe', 'target/debug/bike-osm-ride.exe'));
const output = path.resolve(arg('--out', 'scratch/ui-regressions'));
const port = Number(arg('--port', '9467'));
fs.mkdirSync(output, { recursive: true });
const data = fs.mkdtempSync(path.join(output, 'data-'));
const child = spawn(exe, [], {
  env: { ...process.env, RIDE_DATA_DIR: data, RIDE_DEMO: '1', RIDE_SIM: '220',
    RIDE_TIMESCALE: '50', RIDE_WINDOW: '412x860', RIDE_DEBUG_PORT: String(port),
    RIDE_TILE_BASE: 'http://127.0.0.1:9', RIDE_LANG: 'hu' },
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
});
let log = '';
child.stdout.on('data', b => { log += b; });
child.stderr.on('data', b => { log += b; });
child.on('error', error => { log += String(error); });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket;
try {
  let target;
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) throw new Error(`App exited: ${child.exitCode}`);
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page'); } catch {}
    if (target?.webSocketDebuggerUrl) break;
    await delay(200);
  }
  assert(target?.webSocketDebuggerUrl, 'WebView2 DevTools did not start');
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    pending.get(message.id)?.(message);
  });
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const request = ++id;
    const timer = setTimeout(() => { pending.delete(request); reject(new Error(`${method} timed out`)); }, 10000);
    pending.set(request, message => {
      clearTimeout(timer); pending.delete(request);
      message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result);
    });
    socket.send(JSON.stringify({ id: request, method, params }));
  });
  const evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await call('Emulation.setDeviceMetricsOverride', { width: 412, height: 860, deviceScaleFactor: 1, mobile: true });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.profile-hit') && !!document.querySelector('.map-marker--rider')`)) break;
    await delay(100);
  }
  assert(await evaluate(`!!document.querySelector('.profile-hit')`), 'Phone profile did not mount');
  const pointEvents = (pointerType, names) => `(() => {
    const el = document.querySelector('.profile-hit');
    const r = el.getBoundingClientRect();
    const options = { bubbles: true, clientX: r.x + r.width / 2, clientY: r.y + r.height / 2, pointerId: 1, pointerType: '${pointerType}', isPrimary: true };
    for (const name of ${JSON.stringify(names)}) el.dispatchEvent(name.startsWith('pointer') ? new PointerEvent(name, options) : new MouseEvent(name, options));
  })()`;
  await evaluate(pointEvents('mouse', ['pointermove', 'mouseenter', 'mousemove']));
  await delay(250);
  assert(await evaluate(`!!document.querySelector('.profile-tip')`), 'Mouse hover should show the profile tooltip');
  await evaluate(pointEvents('touch', ['pointerdown', 'pointerup', 'mousemove']));
  await delay(250);
  assert(!await evaluate(`!!document.querySelector('.profile-tip')`), 'Touch release must clear the tooltip, including synthesized mousemove');
  await evaluate(pointEvents('mouse', ['pointermove', 'mousemove']));
  await delay(250);
  assert(await evaluate(`!!document.querySelector('.profile-tip')`), 'Real mouse hover must work again after touch');
  await evaluate(pointEvents('touch', ['pointerdown', 'pointercancel', 'mousemove']));
  await delay(250);
  assert(!await evaluate(`!!document.querySelector('.profile-tip')`), 'Touch cancellation must clear the tooltip');
  assert(await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.compact-tabs button')].find(b => b.textContent.trim() === 'Térkép');
    tab?.click(); return !!tab;
  })()`), 'The phone map tab must be available');
  await delay(350);
  assert(await evaluate(`(() => { const r = document.querySelector('.map').getBoundingClientRect(); return r.width > 100 && r.height > 100; })()`), 'The camera regression must use a visible map');
  await delay(2500); // Let the following camera travel away from its original position.
  assert(await evaluate(`(() => {
    const button = [...document.querySelectorAll('button')].find(b => b.getAttribute('aria-label') === 'Szünet' || b.textContent.trim() === 'Szünet');
    button?.click(); return !!button;
  })()`), 'The simulated ride must be running before the camera regression');
  await delay(350); // Pause the rider so this check measures camera movement only.
  const markerPosition = `(() => { const m = document.querySelector('.map-marker--rider').getBoundingClientRect(); return {x: m.x + m.width/2, y: m.y + m.height/2}; })()`;
  const before = await evaluate(markerPosition);
  await evaluate(`(() => { const el = document.querySelector('.map'); const r = el.getBoundingClientRect(); const p = { bubbles:true, pointerId:2, pointerType:'touch', isPrimary:true, clientX:r.x+r.width/2, clientY:r.y+r.height/2 }; el.dispatchEvent(new PointerEvent('pointerdown',p)); el.dispatchEvent(new PointerEvent('pointerup',p)); })()`);
  await delay(100);
  const after = await evaluate(markerPosition);
  assert(Math.hypot(after.x - before.x, after.y - before.y) < 3, `Tapping a following map moved its camera: ${JSON.stringify({before, after})}`);
  const dimensions = await evaluate(`({ width: innerWidth, height: innerHeight, content: document.scrollingElement.scrollWidth })`);
  assert(dimensions.content <= dimensions.width + 2, `Phone layout overflows: ${JSON.stringify(dimensions)}`);
  const screenshot = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  fs.writeFileSync(path.join(output, 'phone.png'), Buffer.from(screenshot.data, 'base64'));
  fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({ dimensions, before, after, checks: ['phone-layout', 'touch-release', 'touch-cancel', 'mouse-after-touch', 'map-follow-tap'] }, null, 2));
  console.log('Passed phone layout, touch tooltip lifecycle, mouse recovery and following-map tap regressions.');
} finally {
  socket?.close();
  if (child.pid && child.exitCode === null) spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { windowsHide: true });
  fs.writeFileSync(path.join(output, 'app.log'), log);
}
