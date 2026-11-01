// Tiny CDP helper for QA: evaluates an expression in the WebView2 page and prints the result.
//
//   node scripts/cdp_eval.mjs <port> "<javascript expression>"
//
// The app must be started with RIDE_DEBUG_PORT=<port>. Read-only by design (no DOM mutation),
// used to inspect computed layout while tuning the shell; not part of the app.
const [port, rawExpression] = process.argv.slice(2);
if (!port || !rawExpression) {
  console.error('usage: node scripts/cdp_eval.mjs <port> "<expression>"');
  process.exit(2);
}
const expression = rawExpression.startsWith('b64:')
  ? Buffer.from(rawExpression.slice(4), 'base64').toString('utf8')
  : rawExpression;
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find(t => t.type === 'page' && t.webSocketDebuggerUrl);
if (!page) {
  console.error('no page target');
  process.exit(1);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true });
  ws.addEventListener('error', reject, { once: true });
});
const reply = await new Promise((resolve, reject) => {
  const timeout = setTimeout(() => reject(new Error('CDP timeout')), 5000);
  ws.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id === 1) {
      clearTimeout(timeout);
      resolve(message);
    }
  });
  ws.send(JSON.stringify({
    id: 1,
    method: 'Runtime.evaluate',
    params: { expression, returnByValue: true, awaitPromise: true },
  }));
});
ws.close();
if (reply.result?.exceptionDetails) {
  console.error(JSON.stringify(reply.result.exceptionDetails, null, 2));
  process.exit(1);
}
console.log(JSON.stringify(reply.result?.result?.value, null, 2));
