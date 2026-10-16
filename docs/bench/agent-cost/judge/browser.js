#!/usr/bin/env node
// Browser checks for the two tasks a parser can't judge (review 2 B5), run in the grade
// container with Playwright's Chromium against a built site served from a local folder.
//
//   node browser.js BUILT_DIR T8|T9 '<json: {"stockPrefix": "/stocks/", "stocks": [...]}>'
//
// T8: on every stock page whose dividend is unconfirmed, every element with class
//     `badge` must be rendered red (r >= 150, g <= 90, b <= 90) and bold (weight >= 700),
//     as computed by the browser: whatever the CSS mechanism (classes, modules, inline).
// T9: on the home page, typing into the first text/search input must filter the list:
//     a query by name and a query by ticker each leave exactly the matching stocks'
//     links visible; clearing the input shows them all again. No reload allowed.
// T12: on the home page, one button per sector (a <button>, an input of type button, or an
//     element with role=button, whose text is the sector's name), before the first stock
//     link in the page; clicking a sector's button leaves exactly that sector's stocks'
//     links visible. Each sector is tried in turn, without a reload.
// Prints one JSON object: {"ok": bool, "problems": [...], "observed": {...}}.

const http = require("http");
const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const [root, task, specJson] = process.argv.slice(2);
const spec = JSON.parse(specJson);
const TYPES = { ".html": "text/html; charset=utf-8", ".css": "text/css", ".js": "text/javascript",
  ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".xml": "application/xml" };

function serve() {
  return new Promise((resolve) => {
    const srv = http.createServer((req, res) => {
      let p = decodeURIComponent(new URL(req.url, "http://x").pathname);
      let f = path.join(root, p);
      if (!f.startsWith(path.resolve(root))) { res.writeHead(403); return res.end(); }
      if (fs.existsSync(f) && fs.statSync(f).isDirectory()) f = path.join(f, "index.html");
      if (!fs.existsSync(f)) { res.writeHead(404); return res.end("not found"); }
      res.writeHead(200, { "content-type": TYPES[path.extname(f)] || "application/octet-stream" });
      fs.createReadStream(f).pipe(res);
    });
    srv.listen(0, "127.0.0.1", () => resolve(srv));
  });
}

(async () => {
  const problems = [];
  const observed = {};
  const srv = await serve();
  const base = `http://127.0.0.1:${srv.address().port}`;
  const browser = await chromium.launch();
  const page = await browser.newPage();
  // the site must work offline: block anything not served locally
  await page.route("**/*", (r) => (r.request().url().startsWith(base) ? r.continue() : r.abort()));
  try {
    if (task === "T8") {
      for (const s of spec.stocks) {
        const due = s.upcoming && !s.upcoming.confirmed;
        if (!due) continue;
        const url = `${base}${spec.stockPrefix}${encodeURIComponent(s.ticker)}/`;
        await page.goto(url, { waitUntil: "load" });
        const styles = await page.$$eval(".badge", (els) => els.map((e) => {
          const cs = getComputedStyle(e);
          return { color: cs.color, weight: cs.fontWeight, visible: e.checkVisibility() };
        }));
        observed[s.ticker] = styles;
        if (!styles.length) problems.push(`${spec.stockPrefix}${s.ticker}/: no .badge element`);
        for (const st of styles) {
          const m = st.color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
          const [r, g, b] = m ? m.slice(1).map(Number) : [0, 0, 0];
          if (!(r >= 150 && g <= 90 && b <= 90)) problems.push(`${s.ticker}: badge color is ${st.color}, not red`);
          if (!(Number(st.weight) >= 700)) problems.push(`${s.ticker}: badge font-weight is ${st.weight}, not bold`);
          if (!st.visible) problems.push(`${s.ticker}: badge is not visible`);
        }
      }
    } else if (task === "T12") {
      await page.goto(`${base}/`, { waitUntil: "load" });
      let reloaded = false;
      page.on("framenavigated", (f) => { if (f === page.mainFrame()) reloaded = true; });
      const visibleStocks = () => page.$$eval("a[href]", (as, prefix) => as
        .filter((a) => new URL(a.href).pathname.startsWith(prefix) && a.checkVisibility())
        .map((a) => new URL(a.href).pathname), spec.stockPrefix);
      const norm = (xs) => [...new Set(xs.map((x) => (x.endsWith("/") ? x : x + "/")))].sort();
      const sectors = [...new Set(spec.stocks.map((s) => s.sector))];
      for (const sector of sectors) {
        const buttons = page.locator("button, input[type=button], [role=button]").filter({ visible: true });
        const n = await buttons.count();
        let btn = null;
        for (let i = 0; i < n; i++) {
          const b = buttons.nth(i);
          const t = ((await b.textContent()) || (await b.getAttribute("value")) || "").trim();
          if (t.toLowerCase() === sector.toLowerCase()) { btn = b; break; }
        }
        if (!btn) { problems.push(`/: no button for the sector '${sector}'`); continue; }
        const before = await btn.evaluate((el, prefix) => {
          const first = [...document.querySelectorAll("a[href]")].find((a) => new URL(a.href).pathname.startsWith(prefix));
          return !first || !!(el.compareDocumentPosition(first) & Node.DOCUMENT_POSITION_FOLLOWING);
        }, spec.stockPrefix);
        if (!before) problems.push(`/: the '${sector}' button is not above the stock list`);
        await btn.click();
        await page.waitForTimeout(300);
        const got = norm(await visibleStocks());
        const expected = spec.stocks.filter((s) => s.sector === sector).map((s) => `${spec.stockPrefix}${s.ticker}/`).sort();
        observed[sector] = { visible: got, expected };
        if (JSON.stringify(got) !== JSON.stringify(expected)) {
          problems.push(`after clicking '${sector}': visible ${JSON.stringify(got)}, expected ${JSON.stringify(expected)}`);
        }
      }
      if (reloaded) problems.push("the page reloaded on a button click");
    } else if (task === "T9") {
      await page.goto(`${base}/`, { waitUntil: "load" });
      const input = await page.$("input[type=search], input[type=text], input:not([type])");
      if (!input) {
        problems.push("/: no text or search input");
      } else {
        let reloaded = false;
        page.on("framenavigated", (f) => { if (f === page.mainFrame()) reloaded = true; });
        const visibleStocks = () => page.$$eval("a[href]", (as, prefix) => as
          .filter((a) => new URL(a.href).pathname.startsWith(prefix) && a.checkVisibility())
          .map((a) => new URL(a.href).pathname), spec.stockPrefix);
        const want = (pred) => spec.stocks.filter(pred).map((s) => `${spec.stockPrefix}${s.ticker}/`).sort();
        const norm = (xs) => [...new Set(xs.map((x) => (x.endsWith("/") ? x : x + "/")))].sort();
        const target = spec.stocks.find((s) => s.ticker === "PEP") || spec.stocks[1];
        const cases = [
          ["name", target.name.split(" ")[0].toLowerCase(), want((s) => s.name.toLowerCase().includes(target.name.split(" ")[0].toLowerCase()) || s.ticker.toLowerCase().includes(target.name.split(" ")[0].toLowerCase()))],
          ["ticker", "san", want((s) => s.name.toLowerCase().includes("san") || s.ticker.toLowerCase().includes("san"))],
          ["cleared", "", want(() => true)],
        ];
        for (const [label, q, expected] of cases) {
          await input.fill("");
          if (q) await input.type(q, { delay: 20 });
          else await input.dispatchEvent("input");
          await page.waitForTimeout(300);
          const got = norm(await visibleStocks());
          observed[label] = { query: q, visible: got, expected };
          if (JSON.stringify(got) !== JSON.stringify(expected)) {
            problems.push(`query '${q}' (${label}): visible ${JSON.stringify(got)}, expected ${JSON.stringify(expected)}`);
          }
        }
        if (reloaded) problems.push("the page reloaded while typing");
      }
    }
  } catch (e) {
    problems.push(`browser error: ${e.message.split("\n")[0]}`);
  } finally {
    await browser.close();
    srv.close();
  }
  console.log(JSON.stringify({ ok: problems.length === 0, problems, observed }));
})();
