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
// Exercise long, unbroken imported/agent-generated names in both dialogs.
const source = JSON.parse(fs.readFileSync('crates/ride-model/assets/workouts.json', 'utf8'));
const workout = structuredClone(source.workouts[0]);
const longName = 'Supercalifragilisticexpialidocious'.repeat(4);
workout.id = 'import-ui-long-name';
workout.name = longName;
workout.steps[0].name = longName;
const now = new Date();
const today = [now.getFullYear(), String(now.getMonth() + 1).padStart(2, '0'), String(now.getDate()).padStart(2, '0')].join('-');
const entry = { id: 'ui-calendar-long-name', date: today, time: null, workout };
fs.writeFileSync(path.join(data, 'calendar.json'), JSON.stringify([entry]));
fs.mkdirSync(path.join(data, 'ai-drafts'));
fs.writeFileSync(path.join(data, 'ai-drafts', 'ui-long-name.json'), JSON.stringify({
  id: 'ui-long-name',
  kind: { type: 'training', title: longName, workouts: [workout], entries: [entry] },
}));
const child = spawn(exe, [], {
  env: { ...process.env, RIDE_DATA_DIR: data, RIDE_DEMO: '1', RIDE_SIM: '220',
    RIDE_TIMESCALE: '50', RIDE_WORKOUT: 'c-threshold-2x20', RIDE_WINDOW: '412x860', RIDE_DEBUG_PORT: String(port),
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
  // All four pages must remain tappable and inside the profile even on small or sideways phones.
  for (const [width, height] of [[412, 860], [360, 640], [915, 412]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: true });
    await delay(250);
    const pages = await evaluate(`(() => {
      const head = document.querySelector('.profile-head--pages').getBoundingClientRect();
      return [...document.querySelector('.profile-head--pages').querySelectorAll('.profile-pages__dot')].map(el => {
        const r = el.getBoundingClientRect();
        return {x:r.x, right:r.right, width:r.width, height:r.height, headLeft:head.left, headRight:head.right};
      });
    })()`);
    assert.equal(pages.length, 4, `Four profile pages expected at ${width}x${height}`);
    for (const page of pages) assert(page.width >= 44 && page.height >= 44 && page.x >= page.headLeft && page.right <= page.headRight,
      `Profile page target clipped or too small at ${width}x${height}: ${JSON.stringify(page)}`);
    await evaluate(`document.querySelector('.profile-head--pages').querySelectorAll('.profile-pages__dot')[3].click()`);
    await delay(100);
    assert(await evaluate(`document.querySelector('.profile-head--pages').querySelectorAll('.profile-pages__dot')[3].getAttribute('aria-selected') === 'true'`), 'Last profile page must be selectable');
    await evaluate(`document.querySelector('.profile-head--pages').querySelectorAll('.profile-pages__dot')[0].click()`);
    await delay(100);
  }
  await call('Emulation.setDeviceMetricsOverride', { width: 412, height: 860, deviceScaleFactor: 1, mobile: true });
  // The hover lifecycle below exercises the elevation profile, not the workout preview.
  await evaluate(`document.querySelector('.profile-head--pages').querySelectorAll('.profile-pages__dot')[2].click()`);
  await delay(250);
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
  await evaluate(`document.querySelector('[aria-label="Beállítások"]').click()`);
  await delay(250);
  const stepper = await evaluate(`(() => { const r = document.querySelector('.dialog .field__step').getBoundingClientRect(); return {width:r.width,height:r.height}; })()`);
  assert(stepper.width >= 44 && stepper.height >= 44, `Settings stepper target is too small: ${JSON.stringify(stepper)}`);
  await evaluate(`document.querySelector('.dialog [aria-label="Bezárás"]').click()`);
  await call('Emulation.setDeviceMetricsOverride', { width: 360, height: 640, deviceScaleFactor: 1, mobile: true });
  await evaluate(`document.querySelector('[aria-label="Naptár"]').click()`);
  await delay(300);
  const calendarLayout = await evaluate(`(() => {
    const body = document.querySelector('.dialog__body');
    const day = document.querySelector('.shell-calendar-day').getBoundingClientRect();
    const box = body.getBoundingClientRect();
    const overflow = [...body.querySelectorAll('*')].filter(el => el.getBoundingClientRect().right > box.right + 2)
      .map(el => ({ tag: el.tagName, class: el.className, text: (el.innerText || '').slice(0, 80) })).slice(0, 5);
    return { width: body.clientWidth, content: body.scrollWidth, dayWidth: day.width, dayHeight: day.height, overflow,
      longName: document.querySelector('.shell-calendar-entry')?.innerText.includes('Supercalifragilisticexpialidocious') };
  })()`);
  assert(calendarLayout.longName, 'Long scheduled workout must be visible in the calendar');
  assert(calendarLayout.content <= calendarLayout.width + 2, `Calendar content overflows: ${JSON.stringify(calendarLayout)}`);
  assert(calendarLayout.dayWidth >= 44 && calendarLayout.dayHeight >= 44, `Calendar day target is too small: ${JSON.stringify(calendarLayout)}`);
  await evaluate(`document.querySelector('.dialog [aria-label="Bezárás"]').click()`);
  await evaluate(`document.querySelector('[aria-label="Beállítások"]').click()`);
  await delay(250);
  assert(await evaluate(`(() => {
    const button = [...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'AI-agent megnyitása');
    button?.click(); return !!button;
  })()`), 'AI-agent settings button must be available');
  await delay(300);
  const agentLayout = await evaluate(`(() => {
    const body = document.querySelector('.dialog__body');
    return { width: body.clientWidth, content: body.scrollWidth,
      longName: document.querySelector('.shell-agent-draft')?.innerText.includes('Supercalifragilisticexpialidocious') };
  })()`);
  assert(agentLayout.longName, 'Long AI workout proposal must be visible');
  assert(agentLayout.content <= agentLayout.width + 2, `AI proposal content overflows: ${JSON.stringify(agentLayout)}`);
  await evaluate(`document.querySelector('.dialog [aria-label="Bezárás"]').click()`);
  await call('Emulation.setDeviceMetricsOverride', { width: 412, height: 860, deviceScaleFactor: 1, mobile: true });
  const screenshot = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  fs.writeFileSync(path.join(output, 'phone.png'), Buffer.from(screenshot.data, 'base64'));
  fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({ dimensions, before, after, stepper, calendarLayout, agentLayout, checks: ['phone-layout', 'profile-page-targets', 'settings-stepper-target', 'touch-release', 'touch-cancel', 'mouse-after-touch', 'map-follow-tap', 'calendar-long-name', 'agent-long-name'] }, null, 2));
  console.log('Passed phone layout, touch targets, map interactions, and long calendar and AI proposal text regressions.');
} finally {
  socket?.close();
  if (child.pid && child.exitCode === null) spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { windowsHide: true });
  fs.writeFileSync(path.join(output, 'app.log'), log);
}
