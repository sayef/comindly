import { RefreshCcw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Org } from "@/components/org-views";
import { RepoDot } from "@/components/symbol";
import { cn } from "@/lib/utils";

const W = 168;
const H = 50;
const GAP = 16;
const ROW = 104;

interface L {
  /** Code references (imports and calls), apart from contracts. */
  code: number;
  from: string;
  to: string;
  weight: number;
  contract: boolean;
  cycle: boolean;
  via: string[];
}

interface Box {
  id: string;
  x: number;
  y: number;
  line: number;
}

const short = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));

type P = { x: number; y: number };

/** A smooth vertical path through `p`: a plain curve for two points, a B-spline through more. */
function curve(p: P[]) {
  if (p.length === 2) {
    const [a, b] = p;
    const dy = (b.y - a.y) / 2;
    return `M${a.x},${a.y} C${a.x},${a.y + dy} ${b.x},${b.y - dy} ${b.x},${b.y}`;
  }
  let d = `M${p[0].x},${p[0].y} L${(5 * p[0].x + p[1].x) / 6},${(5 * p[0].y + p[1].y) / 6}`;
  for (let i = 1; i < p.length - 1; i++) {
    const [a, b, c] = [p[i - 1], p[i], p[i + 1]];
    d += ` C${(2 * a.x + b.x) / 3},${(2 * a.y + b.y) / 3} ${(a.x + 2 * b.x) / 3},${(a.y + 2 * b.y) / 3} ${(a.x + 4 * b.x + c.x) / 6},${(a.y + 4 * b.y + c.y) / 6}`;
  }
  const [y, z] = [p[p.length - 2], p[p.length - 1]];
  return `${d} C${(2 * y.x + z.x) / 3},${(2 * y.y + z.y) / 3} ${(y.x + 2 * z.x) / 3},${(y.y + 2 * z.y) / 3} ${z.x},${z.y}`;
}

/** Boxes in lines away from the focus; every other line sits half a box over, so its arrows pass
 * through the gaps of the line in front. `y(line)` places each line. */
function rows(ids: string[], per: number, width: number, y: (line: number) => number): Box[] {
  const cell = W + GAP;
  const x0 = (width - (per * W + (per - 1) * GAP)) / 2;
  const out: Box[] = [];
  let i = 0;
  for (let line = 0; i < ids.length; line++) {
    const odd = line % 2 === 1 && per > 1;
    const cap = odd ? per - 1 : per;
    const n = Math.min(cap, ids.length - i);
    const first = Math.floor((cap - n) / 2);
    for (let j = 0; j < n; j++, i++) {
      out.push({ id: ids[i], x: x0 + (odd ? cell / 2 : 0) + (first + j) * cell, y: y(line), line });
    }
  }
  return out;
}

/** How many lines `rows` needs for `n` boxes. */
function lines(n: number, per: number) {
  let used = 0;
  let k = 0;
  while (used < n) used += k++ % 2 === 1 && per > 1 ? per - 1 : per;
  return k;
}

const KIND: Record<string, string> = { http: "HTTP routes", sqs: "Queues", sns: "Topics", s3: "Buckets", table: "Tables" };

/** How two repositories are connected: contracts grouped by type, then code references. */
function LinkCard({ l, x, y, width, height }: { l: L; x: number; y: number; width: number; height: number }) {
  const groups = new Map<string, string[]>();
  for (const v of l.via) {
    const i = v.indexOf(":");
    const k = i > 0 ? v.slice(0, i) : "other";
    groups.set(k, [...(groups.get(k) ?? []), i > 0 ? v.slice(i + 1) : v]);
  }
  const left = Math.min(x + 14, width - 300);
  // Open away from the nearer edge, so the card is never cut off.
  const place = y > height / 2 ? { bottom: height - y + 14 } : { top: y + 14 };
  return (
    <div className="pointer-events-none absolute z-10 max-h-[calc(100%-1.5rem)] w-72 overflow-hidden rounded-lg border bg-popover p-3 text-xs shadow-float" style={{ left, ...place }}>
      <p className="mb-2 text-sm">
        <span className="font-semibold">{l.from}</span> <span className="text-muted-foreground">uses</span> <span className="font-semibold">{l.to}</span>
      </p>
      <div className="flex flex-col gap-2">
        {[...groups].map(([k, names]) => (
          <div key={k}>
            <p className="eyebrow">
              {KIND[k] ?? "Contracts"} <span className="nums font-normal">{names.length}</span>
            </p>
            <ul className="mt-0.5 flex flex-col gap-0.5">
              {names.slice(0, 6).map((n) => (
                <li key={n} className="truncate font-mono text-accent-foreground">
                  {n}
                </li>
              ))}
              {names.length > 6 && <li className="text-muted-foreground">and {names.length - 6} more</li>}
            </ul>
          </div>
        ))}
        {l.code > 0 && (
          <p className="text-muted-foreground">
            <span className="nums font-semibold text-foreground">{l.code}</span> code reference{l.code === 1 ? "" : "s"} (imports and calls)
          </p>
        )}
        {l.cycle && <p className="text-warning">Part of a cycle: these repositories also depend back on each other.</p>}
      </div>
    </div>
  );
}

/** One repository at a time: what uses it above, what it uses below, and the list to move between them. */
export function OrgNetwork({ org }: { org: Org }) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [tip, setTip] = useState<{ l: L; x: number; y: number } | null>(null);

  const links = useMemo(() => {
    const inCycle = (a: string, b: string) => org.cycles.some((c) => c.includes(a) && c.includes(b));
    const out: L[] = [];
    for (const [from, to] of org.uses) {
      for (const [dst, es] of to) {
        if (from === dst) continue;
        out.push({
          from,
          to: dst,
          weight: es.reduce((n, e) => n + (e.weight || 1), 0),
          code: es.filter((e) => e.kind !== "contract").reduce((n, e) => n + (e.weight || 1), 0),
          contract: es.some((e) => e.kind === "contract"),
          cycle: inCycle(from, dst),
          via: [...new Set(es.flatMap((e) => e.via ?? []))],
        });
      }
    }
    return out;
  }, [org]);

  const ranked = useMemo(
    () =>
      org.repos
        .map((id) => ({ id, usedBy: org.usedBy.get(id)?.size ?? 0, uses: org.uses.get(id)?.size ?? 0 }))
        .sort((a, b) => b.usedBy - a.usedBy || b.uses - a.uses || a.id.localeCompare(b.id)),
    [org],
  );
  const [focus, setFocus] = useState<string | null>(null);
  const current = focus && org.repos.includes(focus) ? focus : ranked[0]?.id;
  const inCycle = new Set(org.cycles.flat());

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setWidth(Math.max(280, e.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  if (!current) return <p className="px-5 py-8 text-sm text-muted-foreground">No repositories in the index yet.</p>;

  const up = links.filter((l) => l.to === current).sort((a, b) => b.weight - a.weight);
  const down = links.filter((l) => l.from === current).sort((a, b) => b.weight - a.weight);
  const per = Math.max(1, Math.floor((width - 32 + GAP) / (W + GAP)));
  const upLines = lines(up.length, per);
  const top = 28 + (upLines ? 22 : 0);
  const focusY = top + (upLines ? upLines * ROW : 0);
  const upBoxes = rows(
    up.map((l) => l.from),
    per,
    width,
    (k) => focusY - ROW * (k + 1),
  );
  const downBoxes = rows(
    down.map((l) => l.to),
    per,
    width,
    (k) => focusY + ROW * (k + 1),
  );
  const me: Box = { id: current, x: (width - W) / 2, y: focusY, line: 0 };
  const downLines = lines(down.length, per);
  const height = focusY + H + (downLines ? downLines * ROW + 22 : 0) + 28;

  // Arrivals and departures spread along the focus box's edges, in the order the other boxes sit.
  const spread = (n: number, i: number) => (n === 1 ? 0 : ((i / (n - 1)) * 2 - 1) * (W / 2 - 18));
  const center = (b: Box) => b.x + W / 2;
  // From a box in a back line, pass every line in front through its gaps, stepping half a box towards the middle.
  const lane = (b: Box, dir: 1 | -1): P[] => {
    const out: P[] = [];
    let x = center(b);
    let off = b.line % 2;
    for (let m = b.line - 1; m >= 0; m--) {
      if (off === m % 2 && per > 1) {
        x += (x < width / 2 ? 1 : -1) * ((W + GAP) / 2);
        off ^= 1;
      }
      const lineY = focusY + dir * ROW * (m + 1);
      out.push({ x, y: lineY + (dir === -1 ? -8 : H + 8) }, { x, y: lineY + (dir === -1 ? H + 8 : -8) });
    }
    return out;
  };
  const show = (l: L, e: React.PointerEvent) => {
    const r = box.current!.getBoundingClientRect();
    setTip({ l, x: e.clientX - r.left, y: e.clientY - r.top });
  };

  const edge = (l: L, pts: P[]) => {
    const kind = l.cycle ? "cycle" : l.contract ? "contract" : "code";
    const hot = tip?.l === l;
    return (
      <g key={`${l.from}>${l.to}`}>
        <path
          d={curve(pts)}
          fill="none"
          stroke={kind === "contract" ? "var(--color-brand)" : kind === "cycle" ? "var(--color-warning)" : "var(--color-muted-foreground)"}
          strokeWidth={hot ? 2.5 : 1.5}
          strokeOpacity={hot ? 1 : kind === "code" ? 0.55 : 0.85}
          strokeDasharray={kind === "cycle" ? "6 4" : undefined}
          markerEnd={`url(#tip-${kind})`}
          className="pointer-events-none"
        />
        <path
          d={curve(pts)}
          fill="none"
          stroke="transparent"
          strokeWidth={14}
          className="cursor-help"
          onPointerEnter={(e) => show(l, e)}
          onPointerMove={(e) => show(l, e)}
          onPointerLeave={() => setTip(null)}
        />
      </g>
    );
  };

  const node = (b: Box, main = false) => (
    <g
      key={`${b.id}@${b.y}`}
      tabIndex={main ? -1 : 0}
      role={main ? undefined : "button"}
      aria-label={main ? undefined : `Focus on ${b.id}`}
      transform={`translate(${b.x},${b.y})`}
      className={cn("outline-none [&:focus-visible>rect]:stroke-ring", !main && "cursor-pointer [&:hover>rect]:stroke-muted-foreground")}
      onClick={() => !main && (setFocus(b.id), setTip(null))}
      onKeyDown={(e) => !main && (e.key === "Enter" || e.key === " ") && (e.preventDefault(), setFocus(b.id))}
    >
      <rect
        width={W}
        height={H}
        rx={8}
        fill={main ? "var(--color-accent)" : "var(--color-card)"}
        stroke={main ? "var(--color-brand)" : inCycle.has(b.id) && inCycle.has(current) ? "var(--color-warning)" : "var(--color-input)"}
        strokeWidth={main ? 1.5 : 1}
        className="transition-[stroke] duration-100"
      />
      <text x={12} y={21} className="fill-foreground text-sm font-semibold">
        {b.id.length > 20 ? `${b.id.slice(0, 19)}…` : b.id}
        <title>{b.id}</title>
      </text>
      <text x={12} y={38} className="nums fill-muted-foreground text-xs">
        {short(org.symbols.get(b.id) ?? 0)} symbols
      </text>
    </g>
  );

  return (
    <div className="grid min-h-[360px] md:grid-cols-[240px_minmax(0,1fr)]">
      <nav aria-label="Repositories" className="flex max-h-48 flex-col overflow-y-auto overscroll-contain border-b p-2 md:max-h-none md:border-r md:border-b-0">
        {ranked.map((r) => (
          <button
            key={r.id}
            onClick={() => (setFocus(r.id), setTip(null))}
            aria-current={r.id === current ? "true" : undefined}
            className={cn(
              "flex min-h-8 items-center gap-2 rounded-md px-2 text-left text-sm transition-colors duration-100 hover:bg-muted",
              r.id === current && "bg-accent font-semibold text-accent-foreground hover:bg-accent",
            )}
          >
            <RepoDot repo={r.id} />
            <span className="min-w-0 flex-1 truncate">{r.id}</span>
            {inCycle.has(r.id) && <RefreshCcw className="size-3 text-warning" aria-label="in a cycle" />}
            <span className="nums text-xs font-normal text-muted-foreground" title={`Used by ${r.usedBy}, uses ${r.uses}`}>
              {r.usedBy || "–"}
            </span>
          </button>
        ))}
      </nav>
      <div ref={box} className="relative min-w-0" style={{ height }}>
        <svg width={width} height={height} className="block" role="img" aria-label={`${current}: what uses it and what it uses`}>
          <defs>
            {(["code", "contract", "cycle"] as const).map((k) => (
              <marker key={k} id={`tip-${k}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" markerUnits="userSpaceOnUse" orient="auto">
                <path
                  d="M0,1 L9,5 L0,9 z"
                  fill={k === "contract" ? "var(--color-brand)" : k === "cycle" ? "var(--color-warning)" : "color-mix(in srgb, var(--color-muted-foreground) 70%, transparent)"}
                />
              </marker>
            ))}
          </defs>
          {upLines > 0 && (
            <text x={16} y={18} className="fill-muted-foreground text-xs font-semibold">
              Used by {up.length}
            </text>
          )}
          {downLines > 0 && (
            <text x={16} y={focusY + H + 40} className="fill-muted-foreground text-xs font-semibold">
              Uses {down.length}
            </text>
          )}
          {up.map((l, i) =>
            edge(l, [{ x: center(upBoxes[i]), y: upBoxes[i].y + H }, ...lane(upBoxes[i], -1), { x: center(me) + spread(up.length, i), y: me.y - 3 }]),
          )}
          {down.map((l, i) =>
            edge(l, [{ x: center(me) + spread(down.length, i), y: me.y + H }, ...lane(downBoxes[i], 1).reverse(), { x: center(downBoxes[i]), y: downBoxes[i].y - 3 }]),
          )}
          {upBoxes.map((b) => node(b))}
          {downBoxes.map((b) => node(b))}
          {node(me, true)}
          {!up.length && !down.length && (
            <text x={width / 2} y={me.y + H + 32} textAnchor="middle" className="fill-muted-foreground text-xs">
              No links to other repositories in the index.
            </text>
          )}
        </svg>
        {tip && <LinkCard l={tip.l} x={tip.x} y={tip.y} width={width} height={height} />}
      </div>
    </div>
  );
}
