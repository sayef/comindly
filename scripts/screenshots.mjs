// Captures the web UI's pages in light and dark: node scripts/screenshots.mjs <ui url with token> <out dir>
// With `server` after them, signs in with SHOT_ADMIN_TOKEN and captures the team server's pages.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [url, out, mode] = process.argv.slice(2);
const base = new URL(url).origin;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = [
  process.env.CHROME,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].find((p) => p && existsSync(p));
if (!browser) throw new Error("no Chrome or Edge found; set CHROME");

const profile = mkdtempSync(join(tmpdir(), "comindly-shots-"));
const port = 9300 + Math.floor(Math.random() * 600);
// A mock keychain keeps macOS from asking where to store the browser's secrets.
const flags = ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--use-mock-keychain", "--password-store=basic", "--no-first-run"];
const proc = spawn(browser, [...flags, `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "about:blank"], {
  stdio: "ignore",
  env: { ...process.env, HOME: process.env.REAL_HOME ?? process.env.HOME },
});

let targets;
for (let i = 0; i < 40 && !targets; i++) {
  try {
    targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  } catch {
    await sleep(250);
  }
}
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  pending.get(d.id)?.(d.result ?? d.error);
  pending.delete(d.id);
};
const send = (method, params = {}) => new Promise((r) => (pending.set(++seq, r), ws.send(JSON.stringify({ id: seq, method, params }))));
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result?.value;
const go = async (path, wait = 2500) => (await send("Page.navigate", { url: base + path }), await sleep(wait));
const shot = async (name) => {
  const { data } = await send("Page.captureScreenshot", { format: "webp", quality: 90 });
  writeFileSync(join(out, `${name}.webp`), Buffer.from(data, "base64"));
  console.log(`  ${name}.webp`);
};

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 2, mobile: false });
await send("Page.navigate", { url });
await sleep(2000);
const post = (path, body, credentials = "include") =>
  `fetch(${JSON.stringify(path)}, { method: "POST", credentials: "${credentials}", headers: { "content-type": "application/json" }, body: ${JSON.stringify(JSON.stringify(body))} }).then((r) => r.json())`;

if (mode === "server") {
  await evaluate(post("/api/v1/auth/redeem", { token: process.env.SHOT_ADMIN_TOKEN }));
  // A member and a viewer who joined, one invitation still open, and an agent's token.
  for (const [email, role] of [["sam@acme.example", "member"], ["alex@acme.example", "viewer"]]) {
    const invite = await evaluate(post("/api/v1/members/invite", { email, role }));
    await evaluate(post("/api/v1/auth/code", { email, code: invite.code }, "omit"));
  }
  await evaluate(post("/api/v1/members/invite", { email: "kim@acme.example", role: "member" }));
  await evaluate(post("/api/v1/tokens", { name: "Claude Code on my laptop" }));
  for (const theme of ["light", "dark"]) {
    await evaluate(`localStorage.setItem("comindly.theme", "${theme}")`);
    const suffix = theme === "dark" ? "-dark" : "";
    await go("/settings/members");
    await shot(`members${suffix}`);
    await go("/settings/access");
    await shot(`access${suffix}`);
    // A command line waiting for approval, as `comindly login` opens it.
    const device = await evaluate(post("/api/v1/auth/device/start", { client: "comindly CLI on sam-laptop" }, "omit"));
    await go(`/device?code=${device.user_code}`);
    await shot(`device${suffix}`);
  }
}

const focus = mode === "server" ? null : await evaluate(`fetch("/api/v1/find?q=create_order").then((r) => r.json()).then((j) => (j.results ?? j)[0]?.id)`);

for (const theme of mode === "server" ? [] : ["light", "dark"]) {
  await evaluate(`localStorage.setItem("comindly.theme", "${theme}")`);
  const suffix = theme === "dark" ? "-dark" : "";
  await go("/");
  await shot(`overview${suffix}`);
  if (focus) {
    await go(`/explorer?focus=${encodeURIComponent(focus)}`, 3500);
    await shot(`explorer${suffix}`);
  }
  for (const [page, path] of [["contracts", "contracts"], ["ledger", "ledger"], ["packages", "packages"], ["conventions", "conventions"], ["repositories", "settings/repositories"], ["setup", "setup"]]) {
    await go(`/${path}`);
    await shot(`${page}${suffix}`);
  }
  await go("/flows");
  await shot(`flows${suffix}`);
  const across = await evaluate(`fetch("/api/v1/flows").then((r) => r.json()).then((l) => (l.find((f) => f.repos.length > 2) ?? l[0])?.entry.id)`);
  if (across) {
    await go(`/flows?id=${encodeURIComponent(across)}`, 3000);
    await shot(`flow${suffix}`);
  }
  await go("/conventions");
  await evaluate(`(async () => { document.querySelector('[aria-label="Show where it comes from"]')?.click(); await new Promise((r) => setTimeout(r, 400)); [...document.querySelectorAll("button")].find((b) => b.textContent.includes("Show code"))?.click(); })()`);
  await sleep(1500);
  await shot(`conventions-open${suffix}`);
  await go("/");
  await evaluate(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, ctrlKey: true, bubbles: true }))`);
  await sleep(500);
  await send("Input.insertText", { text: "charge an order" });
  await sleep(2000);
  await shot(`palette${suffix}`);
}

// Last, since it re-indexes: two runs from the app, the second queued behind the first, for the Activity page.
if (mode !== "server") {
  for (let i = 0; i < 2; i++) await evaluate(post("/api/v1/index", { llm: false, drop_unlisted: false }));
  const jobs = `fetch("/api/v1/jobs").then((r) => r.json()).then((j) => j.jobs)`;
  for (let i = 0; i < 120 && (await evaluate(`${jobs}.then((l) => l.some((j) => j.status === "queued" || j.status === "running"))`)); i++) await sleep(1000);
  const first = await evaluate(`${jobs}.then((l) => l[l.length - 1]?.id)`);
  for (const theme of ["light", "dark"]) {
    await evaluate(`localStorage.setItem("comindly.theme", "${theme}")`);
    await go(`/activity/${first}`);
    await evaluate(`[...document.querySelectorAll("button")].find((b) => b.textContent.startsWith("Parsing"))?.click()`);
    await sleep(500);
    await shot(`activity${theme === "dark" ? "-dark" : ""}`);
  }
}

ws.close();
proc.kill();
await sleep(300);
rmSync(profile, { recursive: true, force: true });
