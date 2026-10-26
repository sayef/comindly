// Copy buttons, and the landing page's graph: repositories as clusters, a change rippling out to its callers.

document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-cm-copy], [data-cm-copy-page]");
  if (!button) return;
  const text = button.dataset.cmCopy ?? button.parentElement.querySelector(".cm-source")?.value ?? "";
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    return;
  }
  const label = button.querySelector("span:last-child");
  if (!label) return;
  const before = label.textContent;
  label.textContent = "copied";
  setTimeout(() => (label.textContent = before), 1400);
});

let stopGraph = null;

function startGraph() {
  stopGraph?.();
  stopGraph = null;
  const canvas = document.querySelector(".cm-graph");
  if (canvas) stopGraph = graph(canvas);
}

if (window.document$) document$.subscribe(startGraph);
else document.addEventListener("DOMContentLoaded", startGraph);

function graph(canvas) {
  const REPOS = ["storefront", "orders", "payments", "catalog", "shipping"];
  const LINKS = [[0, 1], [0, 2], [1, 3], [2, 4], [3, 4], [0, 4], [1, 4]];
  const STEP = 420, HOLD = 1100, FADE = 1500, DEPTH = 5;
  const PALE = [194, 241, 227], MINT = [139, 227, 201];
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const ctx = canvas.getContext("2d");
  let W = 0, H = 0, nodes = [], edges = [], adj = [], centers = [], ripples = [];
  let raf = 0, visible = true, nextAt = 0, lastPoke = 0;

  const seeded = (s) => () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(",");

  function build() {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    W = canvas.clientWidth;
    H = canvas.clientHeight;
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const rand = seeded(11);
    const wide = W > 760;
    const spots = wide
      ? [[0.13, 0.3], [0.86, 0.26], [0.17, 0.78], [0.84, 0.76], [0.5, 0.92]]
      : [[0.2, 0.1], [0.8, 0.13], [0.15, 0.9], [0.85, 0.88], [0.5, 0.97]];
    const per = wide ? 26 : 13;
    const rx = wide ? W * 0.085 : W * 0.16, ry = wide ? H * 0.12 : H * 0.07;
    centers = spots.map(([x, y]) => [x * W, y * H]);
    nodes = [];
    edges = [];
    centers.forEach(([cx, cy], repo) => {
      for (let i = 0; i < per; i++) {
        const a = rand() * Math.PI * 2, r = Math.sqrt(rand());
        nodes.push({ x: cx + Math.cos(a) * r * rx, y: cy + Math.sin(a) * r * ry, repo, phase: rand() * 6.3, size: 1.1 + rand() * 1.5 });
      }
    });
    const seen = new Set();
    const link = (a, b, contract) => {
      const key = a < b ? `${a}-${b}` : `${b}-${a}`;
      if (a === b || seen.has(key)) return;
      seen.add(key);
      edges.push({ a, b, contract });
    };
    nodes.forEach((n, i) => {
      nodes
        .map((m, j) => [j, (m.x - n.x) ** 2 + (m.y - n.y) ** 2])
        .filter(([j]) => nodes[j].repo === n.repo && j !== i)
        .sort((p, q) => p[1] - q[1])
        .slice(0, 2)
        .forEach(([j]) => link(i, j, false));
    });
    for (const [r1, r2] of LINKS) {
      for (let k = 0; k < 2; k++) link(r1 * per + Math.floor(rand() * per), r2 * per + Math.floor(rand() * per), true);
    }
    adj = nodes.map(() => []);
    edges.forEach((e, i) => {
      adj[e.a].push([e.b, i]);
      adj[e.b].push([e.a, i]);
    });
    ripples = [];
  }

  function ripple(origin, t0) {
    const depth = new Map([[origin, 0]]);
    const lit = [];
    let frontier = [origin];
    for (let d = 0; d < DEPTH && frontier.length; d++) {
      const next = [];
      for (const n of frontier) {
        for (const [m, e] of adj[n]) {
          if (depth.has(m)) continue;
          depth.set(m, d + 1);
          lit.push({ from: n, to: m, d, edge: e });
          next.push(m);
        }
      }
      frontier = next;
    }
    ripples.push({ origin, t0, depth, lit, end: t0 + (DEPTH + 1) * STEP + HOLD + FADE });
  }

  function crossRepoStart() {
    const hubs = edges.filter((e) => e.contract).map((e) => e.a);
    return hubs[Math.floor(Math.random() * hubs.length)];
  }

  function at(n, t) {
    const p = nodes[n];
    return [p.x + Math.sin(t * 0.0004 + p.phase) * 2, p.y + Math.cos(t * 0.00035 + p.phase) * 2];
  }

  // Links between repositories bow away from the centre, around the headline.
  function bend(e, t) {
    const [ax, ay] = at(e.a, t), [bx, by] = at(e.b, t);
    const mx = (ax + bx) / 2, my = (ay + by) / 2;
    if (!e.contract) return [ax, ay, mx, my, bx, by];
    const dx = mx - W / 2, dy = my - H * 0.48, len = Math.hypot(dx, dy) || 1;
    const k = Math.hypot(bx - ax, by - ay) * 0.22;
    return [ax, ay, mx + (dx / len) * k, my + (dy / len) * k, bx, by];
  }

  function trace(c, from, p) {
    const [ax, ay, cx, cy, bx, by] = c;
    const steps = 18;
    ctx.beginPath();
    for (let i = 0; i <= steps * p; i++) {
      let u = i / steps;
      if (from) u = 1 - u;
      const v = 1 - u;
      const x = v * v * ax + 2 * v * u * cx + u * u * bx, y = v * v * ay + 2 * v * u * cy + u * u * by;
      i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
    }
  }

  function fade(t, reach) {
    const age = t - reach - HOLD;
    return age <= 0 ? 1 : Math.max(0, 1 - age / FADE);
  }

  function draw(t) {
    ctx.clearRect(0, 0, W, H);
    ctx.lineCap = "round";

    for (const e of edges) {
      const [ax, ay, cx, cy, bx, by] = bend(e, t);
      ctx.setLineDash(e.contract ? [3, 6] : []);
      ctx.strokeStyle = e.contract ? "rgba(139,227,201,0.18)" : "rgba(242,242,242,0.09)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.quadraticCurveTo(cx, cy, bx, by);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    const glow = new Map();
    ctx.globalCompositeOperation = "lighter";
    for (const r of ripples) {
      for (const s of r.lit) {
        const start = r.t0 + s.d * STEP;
        const p = Math.min(1, Math.max(0, (t - start) / STEP));
        if (p <= 0) continue;
        const a = fade(t, start + STEP);
        if (a <= 0) continue;
        const rgb = mix(PALE, MINT, s.d / (DEPTH - 1));
        trace(bend(edges[s.edge], t), edges[s.edge].a !== s.from, p);
        ctx.strokeStyle = `rgba(${rgb},${0.18 * a})`;
        ctx.lineWidth = 4;
        ctx.stroke();
        ctx.strokeStyle = `rgba(${rgb},${0.75 * a})`;
        ctx.lineWidth = 1.2;
        ctx.stroke();
        if (p === 1) glow.set(s.to, Math.max(glow.get(s.to) ?? 0, a * (1 - s.d / DEPTH)));
      }
      glow.set(r.origin, Math.max(glow.get(r.origin) ?? 0, fade(t, r.t0)));
    }
    ctx.globalCompositeOperation = "source-over";

    nodes.forEach((n, i) => {
      const [x, y] = at(i, t);
      const g = glow.get(i) ?? 0;
      const base = 0.3 + 0.14 * Math.sin(t * 0.0009 + n.phase);
      ctx.fillStyle = g ? `rgba(${mix(MINT, PALE, g)},${Math.max(base, g)})` : `rgba(242,242,242,${base})`;
      ctx.beginPath();
      ctx.arc(x, y, n.size + g * 1.6, 0, Math.PI * 2);
      ctx.fill();
    });

    for (const r of ripples) {
      const age = t - r.t0;
      if (age < 0 || age > 1600) continue;
      const [x, y] = at(r.origin, t);
      ctx.strokeStyle = `rgba(194,241,227,${0.6 * (1 - age / 1600)})`;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(x, y, 4 + age / 40, 0, Math.PI * 2);
      ctx.stroke();
    }

    ctx.font = '500 10px "Geist Mono", ui-monospace, monospace';
    ctx.textAlign = "center";
    ctx.fillStyle = "rgba(242,242,242,0.32)";
    if (W > 760) centers.forEach(([x, y], i) => ctx.fillText(REPOS[i].toUpperCase().split("").join(" "), x, y + H * 0.12 + 22));
  }

  function frame(t) {
    raf = 0;
    if (!visible) return;
    if (t >= nextAt && ripples.length < 3) {
      ripple(crossRepoStart(), t);
      nextAt = t + 2600 + Math.random() * 1400;
    }
    ripples = ripples.filter((r) => r.end > t);
    draw(t);
    raf = requestAnimationFrame(frame);
  }

  function poke(event) {
    const now = performance.now();
    if (still || now - lastPoke < 700) return;
    const box = canvas.getBoundingClientRect();
    const x = event.clientX - box.left, y = event.clientY - box.top;
    let best = -1, dist = 36 ** 2;
    nodes.forEach((n, i) => {
      const d = (n.x - x) ** 2 + (n.y - y) ** 2;
      if (d < dist) [best, dist] = [i, d];
    });
    if (best >= 0) {
      lastPoke = now;
      ripple(best, now);
    }
  }

  function show() {
    if (still) {
      ripples = [];
      ripple(crossRepoStart(), -STEP * DEPTH);
      draw(0);
    } else if (!raf && visible) {
      raf = requestAnimationFrame(frame);
    }
  }

  build();
  show();
  const sizer = new ResizeObserver(() => {
    build();
    show();
  });
  sizer.observe(canvas);
  const watcher = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    show();
  });
  watcher.observe(canvas);
  const hero = canvas.parentElement;
  hero.addEventListener("pointermove", poke);
  document.fonts?.ready.then(() => still && show());

  return () => {
    cancelAnimationFrame(raf);
    sizer.disconnect();
    watcher.disconnect();
    hero.removeEventListener("pointermove", poke);
  };
}
