// Local imitation of the three Google endpoints used by the Street View panel, for QA without a
// real API key and without network. Node 18+ (no dependencies).
//
//   node scripts/streetview_mock.mjs [port=9300] [--delay=<ms>] [--gap]
//   set RIDE_STREETVIEW_BASE=http://127.0.0.1:9300      (then start the app; loopback only)
//
// Endpoints (same paths and parameter names as Google's, see docs in ride-ui-streetview/src/url.rs):
//   GET /maps/api/streetview/metadata?location=<lat,lng>&radius=<m>&source=outdoor&key=<KEY>
//       -> {"status":"OK","pano_id":"MOCK<lat*1e4>_<lng*1e4>","location":{lat,lng},"date","copyright"}
//   GET /maps/api/streetview?size=<px>&pano=<id>&heading=<deg>&pitch=<deg>&fov=<deg>&key=<KEY>
//       -> the bundled 640x480 JPEG (the opt-in offline download path), BADKEY/QUOTAKEY errors.
//   GET /maps/embed/v1/streetview?key=<KEY>&pano=<id>&heading=<deg>&pitch=<deg>&fov=<deg>[&location=..]
//       -> a small HTML page that SHOWS the pano id, heading, pitch and fov (and a horizon strip
//          that scrolls with the heading), so the plumbing is visibly verified. It contains no key.
//   GET /__stats -> request counters (metadata / image / embed) as JSON, to prove that a repeat
//                   ride with cached pano ids and downloaded images makes no calls.
//                   POST /__reset zeroes them.
//
// Special keys: BADKEY -> REQUEST_DENIED, QUOTAKEY -> OVER_QUERY_LIMIT, HTTP500KEY -> HTTP 500,
// GARBAGEKEY -> HTML instead of JSON. Any other key works. `--gap`: every third 0.01 degree of
// longitude (about 750 m) has no panorama (ZERO_RESULTS). The log masks the key.
import http from 'node:http';
import { readFileSync } from 'node:fs';

const mockJpeg = readFileSync(new URL('../crates/ride-ui-streetview/assets/mock.jpg', import.meta.url));

const args = process.argv.slice(2);
const port = Number(args.find(a => /^\d+$/.test(a)) ?? 9300);
const delayMs = Number((args.find(a => a.startsWith('--delay=')) ?? '--delay=0').split('=')[1]) || 0;
const gaps = args.includes('--gap');

const stats = { metadata: 0, image: 0, embed: 0, other: 0, maxInFlight: 0 };
let inFlight = 0;

const covered = (lat, lng) => !gaps || ((Math.floor(lng * 100) % 3) + 3) % 3 !== 0;
const panoId = (lat, lng) => `MOCK${Math.round(lat * 1e4)}_${Math.round(lng * 1e4)}`;
const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const num = (v, d) => (v !== null && v !== '' && Number.isFinite(Number(v)) ? Number(v) : d);

function maskedLine(req) {
  return `${req.method} ${req.url.replace(/(key=)[^&\s]*/g, '$1***')}`;
}

function embedPage(q) {
  const pano = q.get('pano') ?? '(location)';
  const heading = num(q.get('heading'), 0);
  const pitch = num(q.get('pitch'), 0);
  const fov = num(q.get('fov'), 90);
  const loc = q.get('location') ?? '';
  // The "horizon" strip scrolls by the heading: 360 deg = one strip width.
  const shift = -((heading % 360) + 360) % 360 / 360 * 100;
  const labels = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  const ticks = labels.map((l, i) => `<span style="left:${i * 12.5}%">${l}</span>`).join('');
  return `<!doctype html><html lang="hu"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Street View mock</title>
<style>
  html,body{margin:0;height:100%;font:14px/1.3 system-ui,sans-serif;background:#1b2530;color:#e8eef5;overflow:hidden}
  .sky{position:absolute;inset:0 0 45% 0;background:#4f7aa6}
  .ground{position:absolute;inset:55% 0 0 0;background:#5a5f55}
  .road{position:absolute;left:44%;right:44%;bottom:0;top:55%;background:#3a3d3a;clip-path:polygon(40% 0,60% 0,100% 100%,0 100%)}
  .strip{position:absolute;left:0;top:30%;height:26%;width:200%;transform:translateX(${shift}%);
         background:repeating-linear-gradient(90deg,#33485e 0 6%,#3f5a75 6% 12%)}
  .strip span{position:absolute;top:8px;color:#f4e9c8;font-weight:700}
  .hud{position:absolute;left:8px;top:8px;padding:6px 10px;background:rgba(0,0,0,.72);border:1px solid #8aa;border-radius:2px}
  .hud b{font-size:16px}
  .hud code{color:#ffd27a}
  .tag{position:absolute;right:8px;bottom:6px;font-size:11px;opacity:.8}
</style></head><body>
<div class="sky"></div><div class="strip">${ticks}${ticks.replace(/left:([\d.]+)%/g, (_, n) => `left:${Number(n) + 100}%`)}</div>
<div class="ground"></div><div class="road"></div>
<div class="hud"><b>MOCK Street View</b><br>pano: <code>${esc(pano)}</code><br>heading: <code>${esc(heading)}&deg;</code>
 pitch: <code>${esc(pitch)}&deg;</code> fov: <code>${esc(fov)}&deg;</code>${loc ? `<br>location: <code>${esc(loc)}</code>` : ''}</div>
<div class="tag">local mock, not Google</div></body></html>`;
}

const server = http.createServer((req, res) => {
  inFlight += 1;
  stats.maxInFlight = Math.max(stats.maxInFlight, inFlight);
  const done = () => { inFlight -= 1; };
  res.on('close', done);
  const url = new URL(req.url, 'http://127.0.0.1');
  const q = url.searchParams;
  const key = q.get('key') ?? '';
  const send = (code, type, body, wait = 0) => setTimeout(() => {
    res.writeHead(code, { 'Content-Type': type, 'Cache-Control': 'no-store' });
    res.end(body);
  }, wait);
  console.log(maskedLine(req));

  if (url.pathname === '/maps/api/streetview/metadata') {
    stats.metadata += 1;
    const json = o => send(200, 'application/json; charset=utf-8', JSON.stringify(o), delayMs);
    if (key === 'BADKEY') return json({ error_message: 'The provided API key is invalid.', status: 'REQUEST_DENIED' });
    if (key === 'QUOTAKEY') return json({ status: 'OVER_QUERY_LIMIT' });
    if (key === 'HTTP500KEY') return send(500, 'application/json', '{}', delayMs);
    if (key === 'GARBAGEKEY') return send(200, 'text/html', '<html>nope</html>', delayMs);
    const m = /^\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*$/.exec(q.get('location') ?? '');
    if (!m) return json({ status: 'INVALID_REQUEST' });
    const lat = Number(m[1]);
    const lng = Number(m[2]);
    if (!covered(lat, lng)) return json({ status: 'ZERO_RESULTS' });
    return json({ copyright: '© Mock', date: '2021-06', location: { lat: Number(lat.toFixed(6)), lng: Number(lng.toFixed(6)) }, pano_id: panoId(lat, lng), status: 'OK' });
  }
  if (url.pathname === '/maps/api/streetview') {
    stats.image += 1;
    if (key === 'BADKEY') return send(403, 'application/json', '{"status":"REQUEST_DENIED"}', delayMs);
    if (key === 'QUOTAKEY') return send(429, 'application/json', '{"status":"OVER_QUERY_LIMIT"}', delayMs);
    if (key === 'HTTP500KEY') return send(500, 'application/json', '{}', delayMs);
    return setTimeout(() => {
      res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Cache-Control': 'no-store' });
      res.end(mockJpeg);
    }, delayMs);
  }
  if (url.pathname === '/maps/embed/v1/streetview') {
    stats.embed += 1;
    return send(200, 'text/html; charset=utf-8', embedPage(q));
  }
  if (url.pathname === '/__stats') return send(200, 'application/json', JSON.stringify(stats));
  if (url.pathname === '/__reset') {
    Object.assign(stats, { metadata: 0, image: 0, embed: 0, other: 0, maxInFlight: 0 });
    return send(200, 'application/json', '{}');
  }
  stats.other += 1;
  return send(404, 'text/plain', 'not found');
});

server.listen(port, '127.0.0.1', () => {
  console.log(`mock Street View on http://127.0.0.1:${port}  (delay ${delayMs} ms, gaps ${gaps ? 'on' : 'off'}; Ctrl+C to stop)`);
});
