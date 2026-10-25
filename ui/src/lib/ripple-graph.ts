// Repositories as clusters of symbols; a change ripples out to its callers. The docs landing page draws the same.

type Rgb = [number, number, number];
type Edge = { a: number; b: number; contract: boolean };
type Lit = { from: number; to: number; d: number; edge: number };
type Ripple = { origin: number; t0: number; lit: Lit[]; end: number };

export type Palette = { node: Rgb; near: Rgb; far: Rgb; contract: Rgb; strength: number; lit: number; glow: boolean };

export const DARK: Palette = { node: [243, 246, 242], near: [163, 230, 53], far: [45, 212, 191], contract: [45, 212, 191], strength: 0.55, lit: 0.35, glow: false };
export const LIGHT: Palette = { node: [11, 17, 14], near: [77, 154, 18], far: [13, 148, 136], contract: [13, 148, 136], strength: 0.45, lit: 0.35, glow: false };

const REPOS = ["storefront", "orders", "payments", "catalog", "shipping"];
const LINKS = [[0, 1], [0, 2], [1, 3], [2, 4], [3, 4], [0, 4], [1, 4]];
const STEP = 420, HOLD = 1100, FADE = 1500, DEPTH = 5;

/** Draws into `canvas` until the returned function is called. */
export function rippleGraph(canvas: HTMLCanvasElement, palette: () => Palette, labels = true): () => void {
  const ctx = canvas.getContext("2d")!;
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let W = 0, H = 0, wide = true;
  let nodes: { x: number; y: number; phase: number; size: number }[] = [];
  let edges: Edge[] = [], adj: [number, number][][] = [], centers: [number, number][] = [], ripples: Ripple[] = [];
  let raf = 0, visible = true, nextAt = 0, lastPoke = 0;

  const seeded = (s: number) => () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  const mix = (a: Rgb, b: Rgb, t: number) => a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(",");
  const rgba = (c: Rgb, a: number) => `rgba(${c.join(",")},${a})`;

  function build() {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    W = canvas.clientWidth;
    H = canvas.clientHeight;
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const rand = seeded(11);
    wide = W > 760;
    const spots = wide
      ? [[0.13, 0.3], [0.86, 0.26], [0.17, 0.78], [0.84, 0.76], [0.5, 0.92]]
      : [[0.2, 0.1], [0.8, 0.13], [0.15, 0.9], [0.85, 0.88], [0.5, 0.97]];
    const per = wide ? 26 : 13;
    const rx = wide ? W * 0.085 : W * 0.16, ry = wide ? H * 0.12 : H * 0.07;
    centers = spots.map(([x, y]) => [x * W, y * H]);
    nodes = [];
    edges = [];
    centers.forEach(([cx, cy]) => {
      for (let i = 0; i < per; i++) {
        const a = rand() * Math.PI * 2, r = Math.sqrt(rand());
        nodes.push({ x: cx + Math.cos(a) * r * rx, y: cy + Math.sin(a) * r * ry, phase: rand() * 6.3, size: 1.1 + rand() * 1.5 });
      }
    });
    const seen = new Set<string>();
    const link = (a: number, b: number, contract: boolean) => {
      const key = a < b ? `${a}-${b}` : `${b}-${a}`;
      if (a === b || seen.has(key)) return;
      seen.add(key);
      edges.push({ a, b, contract });
    };
    nodes.forEach((n, i) => {
      const repo = Math.floor(i / per);
      nodes
        .map((m, j) => [j, (m.x - n.x) ** 2 + (m.y - n.y) ** 2] as const)
        .filter(([j]) => Math.floor(j / per) === repo && j !== i)
        .sort((p, q) => p[1] - q[1])
        .slice(0, 2)
        .forEach(([j]) => link(i, j, false));
    });
    for (const [r1, r2] of LINKS) for (let k = 0; k < 2; k++) link(r1 * per + Math.floor(rand() * per), r2 * per + Math.floor(rand() * per), true);
    adj = nodes.map(() => []);
    edges.forEach((e, i) => {
      adj[e.a].push([e.b, i]);
      adj[e.b].push([e.a, i]);
    });
    ripples = [];
  }

  function ripple(origin: number, t0: number) {
    const seen = new Set([origin]);
    const lit: Lit[] = [];
    let frontier = [origin];
    for (let d = 0; d < DEPTH && frontier.length; d++) {
      const next: number[] = [];
      for (const n of frontier)
        for (const [m, e] of adj[n]) {
          if (seen.has(m)) continue;
          seen.add(m);
          lit.push({ from: n, to: m, d, edge: e });
          next.push(m);
        }
      frontier = next;
    }
    ripples.push({ origin, t0, lit, end: t0 + (DEPTH + 1) * STEP + HOLD + FADE });
  }

  const crossRepoStart = () => {
    const hubs = edges.filter((e) => e.contract).map((e) => e.a);
    return hubs[Math.floor(Math.random() * hubs.length)];
  };

  const at = (n: number, t: number): [number, number] => {
    const p = nodes[n];
    return [p.x + Math.sin(t * 0.0004 + p.phase) * 2, p.y + Math.cos(t * 0.00035 + p.phase) * 2];
  };

  // Links between repositories bow away from the centre, around whatever sits there.
  function bend(e: Edge, t: number) {
    const [ax, ay] = at(e.a, t), [bx, by] = at(e.b, t);
    const mx = (ax + bx) / 2, my = (ay + by) / 2;
    if (!e.contract) return [ax, ay, mx, my, bx, by];
    const dx = mx - W / 2, dy = my - H * 0.48, len = Math.hypot(dx, dy) || 1;
    const k = Math.hypot(bx - ax, by - ay) * 0.22;
    return [ax, ay, mx + (dx / len) * k, my + (dy / len) * k, bx, by];
  }

  function trace(c: number[], reverse: boolean, p: number) {
    const [ax, ay, cx, cy, bx, by] = c;
    ctx.beginPath();
    for (let i = 0; i <= 18 * p; i++) {
      const u = reverse ? 1 - i / 18 : i / 18, v = 1 - u;
      const x = v * v * ax + 2 * v * u * cx + u * u * bx, y = v * v * ay + 2 * v * u * cy + u * u * by;
      if (i) ctx.lineTo(x, y);
      else ctx.moveTo(x, y);
    }
  }

  const fade = (t: number, reach: number) => {
    const age = t - reach - HOLD;
    return age <= 0 ? 1 : Math.max(0, 1 - age / FADE);
  };

  function draw(t: number) {
    const p = palette();
    const s = p.strength;
    ctx.clearRect(0, 0, W, H);
    ctx.lineCap = "round";
    for (const e of edges) {
      const [ax, ay, cx, cy, bx, by] = bend(e, t);
      ctx.setLineDash(e.contract ? [3, 6] : []);
      ctx.strokeStyle = e.contract ? rgba(p.contract, 0.18 * s) : rgba(p.node, 0.09 * s);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.quadraticCurveTo(cx, cy, bx, by);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    const glow = new Map<number, number>();
    for (const r of ripples) {
      for (const l of r.lit) {
        const start = r.t0 + l.d * STEP;
        const prog = Math.min(1, Math.max(0, (t - start) / STEP));
        if (prog <= 0) continue;
        const a = fade(t, start + STEP);
        if (a <= 0) continue;
        const rgb = mix(p.near, p.far, l.d / (DEPTH - 1));
        trace(bend(edges[l.edge], t), edges[l.edge].a !== l.from, prog);
        if (p.glow) {
          ctx.strokeStyle = `rgba(${rgb},${0.18 * a * s * p.lit})`;
          ctx.lineWidth = 4;
          ctx.stroke();
        }
        ctx.strokeStyle = `rgba(${rgb},${0.75 * a * s * p.lit})`;
        ctx.lineWidth = 1.2;
        ctx.stroke();
        if (prog === 1) glow.set(l.to, Math.max(glow.get(l.to) ?? 0, a * (1 - l.d / DEPTH)));
      }
      glow.set(r.origin, Math.max(glow.get(r.origin) ?? 0, fade(t, r.t0)));
    }

    nodes.forEach((n, i) => {
      const [x, y] = at(i, t);
      const g = glow.get(i) ?? 0;
      const base = (0.3 + 0.14 * Math.sin(t * 0.0009 + n.phase)) * s;
      ctx.fillStyle = g ? `rgba(${mix(p.far, p.near, g)},${Math.max(base, g * s)})` : rgba(p.node, base);
      ctx.beginPath();
      ctx.arc(x, y, n.size + g * 1.6, 0, Math.PI * 2);
      ctx.fill();
    });

    for (const r of ripples) {
      const age = t - r.t0;
      if (age < 0 || age > 1600) continue;
      const [x, y] = at(r.origin, t);
      ctx.strokeStyle = rgba(p.near, 0.6 * (1 - age / 1600) * s);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(x, y, 4 + age / 40, 0, Math.PI * 2);
      ctx.stroke();
    }

    if (labels && wide) {
      ctx.font = '500 10px "Geist Mono Variable", ui-monospace, monospace';
      ctx.textAlign = "center";
      ctx.fillStyle = rgba(p.node, 0.32 * s);
      centers.forEach(([x, y], i) => ctx.fillText(REPOS[i].toUpperCase().split("").join(" "), x, y + H * 0.12 + 22));
    }
  }

  function frame(t: number) {
    raf = 0;
    if (!visible) return;
    if (t >= nextAt && !ripples.length) {
      ripple(crossRepoStart(), t);
      nextAt = t + 5000 + Math.random() * 2000;
    }
    ripples = ripples.filter((r) => r.end > t);
    draw(t);
    raf = requestAnimationFrame(frame);
  }

  function poke(event: PointerEvent) {
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
    } else if (!raf && visible) raf = requestAnimationFrame(frame);
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
  const host = canvas.parentElement!;
  host.addEventListener("pointermove", poke);
  return () => {
    cancelAnimationFrame(raf);
    sizer.disconnect();
    watcher.disconnect();
    host.removeEventListener("pointermove", poke);
  };
}
