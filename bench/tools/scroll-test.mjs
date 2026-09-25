// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Scroll smoothness test (F1 acceptance): run the app with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev -- -- <file.pdf>
// then: node bench/tools/scroll-test.mjs [px-per-second] [seconds]
//
// Drives the running app over CDP: scrolls the viewer at a constant speed and
// measures frame intervals and frames showing a visible page with no tiles.
const [speed = "2000", seconds = "8"] = process.argv.slice(2);
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((t) => t.type === "page" && t.url.includes("localhost:1420"));
if (!page) throw new Error(`app page not found: ${JSON.stringify(targets.map((t) => t.url))}`);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const call = (method, params) =>
  new Promise((resolve) => {
    const my = ++id;
    const onMsg = (e) => {
      const m = JSON.parse(e.data);
      if (m.id === my) {
        ws.removeEventListener("message", onMsg);
        resolve(m.result);
      }
    };
    ws.addEventListener("message", onMsg);
    ws.send(JSON.stringify({ id: my, method, params }));
  });
const expression = `(async () => {
  const sc = document.querySelector('.viewer-scroll');
  sc.scrollTop = 0;
  await new Promise(r => setTimeout(r, 1000));
  const frames = []; let blank = 0; const t0 = performance.now(); let last = t0;
  await new Promise(resolve => {
    function step(t) {
      frames.push(t - last); last = t;
      sc.scrollTop = (t - t0) / 1000 * ${speed};
      const r = sc.getBoundingClientRect();
      for (const p of sc.querySelectorAll('.page')) {
        const b = p.getBoundingClientRect();
        if (b.bottom > r.top && b.top < r.bottom && !p.querySelector('canvas')) { blank++; break; }
      }
      if (t - t0 < ${seconds} * 1000) requestAnimationFrame(step); else resolve();
    }
    requestAnimationFrame(step);
  });
  frames.shift();
  const s = [...frames].sort((a, b) => a - b);
  const q = (p) => +s[Math.min(s.length - 1, Math.floor(s.length * p))].toFixed(2);
  return JSON.stringify({ speedPxPerS: ${speed}, frames: frames.length, fps: +(frames.length / ${seconds}).toFixed(1),
    p50ms: q(0.5), p95ms: q(0.95), p99ms: q(0.99), maxMs: +s.at(-1).toFixed(2),
    framesOver20ms: frames.filter(f => f > 20).length, blankFrames: blank, dpr: devicePixelRatio,
    endPage: document.querySelector('.toolbar-page')?.textContent });
})()`;
const res = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
console.log(res.result?.value ?? JSON.stringify(res));
ws.close();
