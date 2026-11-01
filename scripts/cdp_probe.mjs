// Dev tool: talk to the running app's WebView2 through the Chrome DevTools Protocol.
// Start the app with RIDE_DEBUG_PORT=<port>, then:
//   node scripts/cdp_probe.mjs <out.png> [port=9222] [--dom]
// Saves a screenshot of the page (reliable, unlike copying the screen) and prints console
// errors. With --dom it also prints the layout tree (element, class, rectangle, background).
import fs from 'node:fs';

const args = process.argv.slice(2).filter(a => !a.startsWith('--'));
const wantDom = process.argv.includes('--dom');
const out = args[0] || 'probe.png';
const port = args[1] || '9222';

const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = list.find(t => t.type === 'page');
if (!page) { console.log('NO PAGE TARGET'); process.exit(2); }

const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const messages = [];
ws.onmessage = ev => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
  if (m.method === 'Runtime.consoleAPICalled') {
    messages.push(`[console.${m.params.type}] ` + m.params.args.map(a => a.value ?? a.description ?? '').join(' '));
  } else if (m.method === 'Runtime.exceptionThrown') {
    messages.push('[exception] ' + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
  } else if (m.method === 'Log.entryAdded') {
    messages.push(`[log.${m.params.entry.level}] ${m.params.entry.text} ${m.params.entry.url || ''}`);
  }
};
const send = (method, params = {}) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
await new Promise(r => (ws.onopen = r));
await send('Runtime.enable');
await send('Log.enable');
await new Promise(r => setTimeout(r, 600));

if (wantDom) {
  const r = await send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => { const d = (el, n) => { const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
      const o = { t: el.tagName.toLowerCase(), c: String(el.className?.baseVal ?? el.className).slice(0, 50), r: [r.x, r.y, r.width, r.height].map(Math.round), bg: cs.backgroundColor };
      if (n > 0) o.k = [...el.children].slice(0, 14).map(k => d(k, n - 1)); return o; };
      return JSON.stringify({ viewport: [innerWidth, innerHeight], text: document.body.innerText.slice(0, 600), tree: d(document.body, 4) }); })()`,
  });
  console.log(r.result?.result?.value);
}
const real = messages.filter(l => !/favicon\.ico/.test(l));
console.log(`console problems: ${real.length}`);
real.slice(0, 25).forEach(l => console.log('  ' + l.slice(0, 300)));

const shot = await send('Page.captureScreenshot', { format: 'png' });
if (shot.result?.data) { fs.writeFileSync(out, Buffer.from(shot.result.data, 'base64')); console.log('saved ' + out); }
ws.close();
process.exit(0);
