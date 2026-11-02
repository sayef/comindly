import { Graph, layout } from "@dagrejs/dagre";
import { Maximize, Minus, Plus, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Org } from "@/components/org-views";
import { RepoDot } from "@/components/symbol";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const W = 184;
const H = 52;
const MIN_HEIGHT = 360;
const MAX_HEIGHT = 640;

interface L {
  /** Code references (imports and calls), apart from contracts. */
  code: number;
  from: string;
  to: string;
  weight: number;
  contract: boolean;
  cycle: boolean;
  via: string[];
  points: { x: number; y: number }[];
}

const short = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));

/** A smooth path through dagre's points, leaving and entering each box horizontally. */
function curve(p: { x: number; y: number }[]) {
  if (p.length < 2) return "";
  let d = `M${p[0].x},${p[0].y}`;
  for (let i = 1; i < p.length; i++) {
    const a = p[i - 1];
    const b = p[i];
    const dx = (b.x - a.x) / 2;
    d += ` C${a.x + dx},${a.y} ${b.x - dx},${b.y} ${b.x},${b.y}`;
  }
  return d;
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

/** Repositories laid out left to right, from what starts work to the shared code it ends up in. */
export function OrgNetwork({ org }: { org: Org }) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);
  const [view, setView] = useState({ k: 1, x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const pan = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);

  const { nodes, links, alone, size } = useMemo(() => {
    const linked = new Set<string>();
    const raw: Omit<L, "points">[] = [];
    const inCycle = (a: string, b: string) => org.cycles.some((c) => c.includes(a) && c.includes(b));
    for (const [from, out] of org.uses) {
      for (const [to, es] of out) {
        if (from === to) continue;
        linked.add(from).add(to);
        raw.push({
          from,
          to,
          weight: es.reduce((n, e) => n + (e.weight || 1), 0),
          code: es.filter((e) => e.kind !== "contract").reduce((n, e) => n + (e.weight || 1), 0),
          contract: es.some((e) => e.kind === "contract"),
          cycle: inCycle(from, to),
          via: [...new Set(es.flatMap((e) => e.via ?? []))],
        });
      }
    }
    const g = new Graph();
    g.setGraph({ rankdir: "LR", ranksep: 110, nodesep: 22, marginx: 24, marginy: 24 });
    g.setDefaultEdgeLabel(() => ({}));
    for (const r of org.repos) if (linked.has(r)) g.setNode(r, { width: W, height: H });
    for (const l of raw) g.setEdge(l.from, l.to, { weight: Math.max(1, Math.log2(l.weight + 1)) });
    layout(g);
    const nodes = g.nodes().map((id) => {
      const n = g.node(id);
      return { id, x: n.x, y: n.y, symbols: org.symbols.get(id) ?? 0 };
    });
    // Leave from the middle of the right side; arrive spread down the left side, in the order sources sit.
    const at = new Map(nodes.map((n) => [n.id, n]));
    const slot = new Map<string, number>();
    for (const n of nodes) {
      const ins = raw.filter((l) => l.to === n.id).sort((a, b) => at.get(a.from)!.y - at.get(b.from)!.y);
      ins.forEach((l, i) => slot.set(`${l.from}>${l.to}`, ins.length === 1 ? 0 : ((i / (ins.length - 1)) * 2 - 1) * (H / 2 - 10)));
    }
    const links: L[] = raw.map((l) => {
      const a = at.get(l.from)!;
      const b = at.get(l.to)!;
      const mid = (g.edge(l.from, l.to)?.points ?? []).slice(1, -1);
      const forward = b.x > a.x;
      const start = { x: a.x + (forward ? W / 2 : -W / 2), y: a.y };
      const end = { x: b.x + (forward ? -W / 2 - 3 : W / 2 + 3), y: b.y + (slot.get(`${l.from}>${l.to}`) ?? 0) };
      return { ...l, points: [start, ...mid.filter((p: { x: number; y: number }) => (forward ? p.x > start.x && p.x < end.x : true)), end] };
    });
    const info = g.graph();
    return {
      nodes,
      links,
      alone: org.repos.filter((r) => !linked.has(r)),
      size: { w: info.width ?? 0, h: info.height ?? 0 },
    };
  }, [org]);

  const height = Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, size.h + 48)));

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setWidth(Math.max(320, e.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const fit = () => {
    if (!size.w) return;
    const k = Math.min(1.2, (width - 32) / size.w, (height - 32) / size.h);
    setView({ k, x: (width - size.w * k) / 2, y: (height - size.h * k) / 2 });
  };
  useEffect(fit, [size.w, size.h, width, height]);

  const zoom = (f: number) =>
    setView((v) => {
      const k = Math.min(3, Math.max(0.25, v.k * f));
      const cx = width / 2;
      const cy = height / 2;
      return { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k };
    });

  const focus = hover ?? picked;
  const near = useMemo(() => {
    if (!focus) return null;
    const s = new Set([focus]);
    for (const l of links) {
      if (l.from === focus) s.add(l.to);
      if (l.to === focus) s.add(l.from);
    }
    return s;
  }, [focus, links]);
  const [tip, setTip] = useState<{ l: L; x: number; y: number } | null>(null);
  const showTip = (l: L, e: React.PointerEvent) => {
    const r = box.current!.getBoundingClientRect();
    setTip({ l, x: e.clientX - r.left, y: e.clientY - r.top });
  };

  const uses = picked ? links.filter((l) => l.from === picked) : [];
  const usedBy = picked ? links.filter((l) => l.to === picked) : [];

  if (nodes.length === 0) return <p className="px-5 py-8 text-sm text-muted-foreground">No links between the indexed repositories yet.</p>;

  return (
    <div>
      <div
        ref={box}
        className="relative touch-none overflow-hidden select-none"
        style={{ height }}
        onWheel={(e) => {
          if (!e.ctrlKey && !e.metaKey) return;
          e.preventDefault();
          zoom(e.deltaY < 0 ? 1.1 : 1 / 1.1);
        }}
        onPointerDown={(e) => {
          if ((e.target as Element).closest("[data-node],[data-panel],[data-controls]")) return;
          pan.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
          (e.currentTarget as Element).setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          const p = pan.current;
          if (p) setView((v) => ({ ...v, x: p.vx + e.clientX - p.x, y: p.vy + e.clientY - p.y }));
        }}
        onPointerUp={() => (pan.current = null)}
      >
        <svg width={width} height={height} className="block" role="img" aria-label="Repositories and the dependencies between them">
          <defs>
            {(["code", "contract", "cycle", "lit"] as const).map((k) => (
              <marker key={k} id={`tip-${k}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" markerUnits="userSpaceOnUse" orient="auto">
                <path
                  d="M0,1 L9,5 L0,9 z"
                  fill={k === "contract" ? "var(--color-brand)" : k === "cycle" ? "var(--color-warning)" : k === "lit" ? "var(--color-foreground)" : "var(--color-input)"}
                />
              </marker>
            ))}
          </defs>
          <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
            {links
              .slice()
              .sort((a, b) => Number(focus === a.from || focus === a.to) - Number(focus === b.from || focus === b.to))
              .map((l) => {
                const lit = !!focus && (l.from === focus || l.to === focus);
                const kind = l.cycle ? "cycle" : l.contract ? "contract" : "code";
                const stroke =
                  kind === "contract" ? "var(--color-brand)" : kind === "cycle" ? "var(--color-warning)" : lit ? "var(--color-foreground)" : "var(--color-input)";
                const hot = tip?.l === l;
                return (
                  <g key={`${l.from}>${l.to}`}>
                    <path
                      d={curve(l.points)}
                      fill="none"
                      stroke={hot && kind === "code" ? "var(--color-foreground)" : stroke}
                      strokeWidth={hot ? 2.5 : 1.5}
                      strokeOpacity={focus && !lit && !hot ? 0.12 : lit || hot || kind === "code" ? 1 : 0.8}
                      strokeDasharray={kind === "cycle" ? "6 4" : undefined}
                      strokeLinecap="round"
                      markerEnd={`url(#tip-${kind === "code" && (lit || hot) ? "lit" : kind})`}
                      className="pointer-events-none transition-[stroke-opacity] duration-150"
                    />
                    <path
                      d={curve(l.points)}
                      fill="none"
                      stroke="transparent"
                      strokeWidth={14}
                      className="cursor-help"
                      onPointerEnter={(e) => showTip(l, e)}
                      onPointerMove={(e) => showTip(l, e)}
                      onPointerLeave={() => setTip(null)}
                    />
                  </g>
                );
              })}
            {nodes.map((n) => {
              const on = n.id === picked || n.id === hover;
              const dim = near && !near.has(n.id);
              return (
                <g
                  key={n.id}
                  data-node
                  tabIndex={0}
                  role="button"
                  aria-label={`${n.id}, ${n.symbols.toLocaleString()} symbols`}
                  aria-pressed={n.id === picked}
                  transform={`translate(${n.x - W / 2},${n.y - H / 2})`}
                  className={cn("cursor-pointer outline-none transition-opacity duration-150 [&:focus-visible>rect]:stroke-ring", dim && "opacity-25")}
                  onPointerEnter={() => setHover(n.id)}
                  onPointerLeave={() => setHover(null)}
                  onFocus={() => setHover(n.id)}
                  onBlur={() => setHover(null)}
                  onClick={() => setPicked((p) => (p === n.id ? null : n.id))}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setPicked((p) => (p === n.id ? null : n.id));
                    }
                    if (e.key === "Escape") setPicked(null);
                  }}
                >
                  <rect
                    width={W}
                    height={H}
                    rx={8}
                    fill={on ? "var(--color-accent)" : "var(--color-card)"}
                    stroke={on ? "var(--color-brand)" : "var(--color-input)"}
                    strokeWidth={on ? 1.5 : 1}
                    className="transition-[fill,stroke] duration-150"
                  />
                  <text x={14} y={22} className="fill-foreground text-sm font-semibold">
                    {n.id.length > 22 ? `${n.id.slice(0, 21)}…` : n.id}
                  </text>
                  <text x={14} y={39} className="nums fill-muted-foreground text-xs">
                    {short(n.symbols)} symbols
                  </text>
                </g>
              );
            })}
          </g>
        </svg>

        <div data-controls className="absolute bottom-3 left-3 flex overflow-hidden rounded-md border bg-popover shadow-float">
          <Button variant="ghost" size="icon" className="size-8 rounded-none" aria-label="Zoom in" onClick={() => zoom(1.25)}>
            <Plus />
          </Button>
          <Button variant="ghost" size="icon" className="size-8 rounded-none" aria-label="Zoom out" onClick={() => zoom(0.8)}>
            <Minus />
          </Button>
          <Button variant="ghost" size="icon" className="size-8 rounded-none" aria-label="Fit to view" onClick={fit}>
            <Maximize />
          </Button>
        </div>

        {tip && <LinkCard l={tip.l} x={tip.x} y={tip.y} width={width} height={height} />}

        {picked && (
          <div data-panel className="absolute top-3 right-3 bottom-3 flex w-80 max-w-[calc(100%-1.5rem)] flex-col overflow-hidden rounded-lg border bg-popover shadow-float">
            <div className="flex items-center gap-2 border-b px-4 py-3">
              <span className="min-w-0 flex-1 truncate font-semibold">{picked}</span>
              <span className="nums text-xs text-muted-foreground">{(org.symbols.get(picked) ?? 0).toLocaleString()} symbols</span>
              <Button variant="ghost" size="icon" className="size-7" aria-label="Close" onClick={() => setPicked(null)}>
                <X />
              </Button>
            </div>
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto overscroll-contain p-4">
              {(
                [
                  ["Uses", uses, (l: L) => l.to],
                  ["Used by", usedBy, (l: L) => l.from],
                ] as const
              ).map(([title, list, other]) => {
                const rows = list.slice().sort((a, b) => b.weight - a.weight);
                return (
                  <section key={title} className="flex flex-col gap-1.5">
                    <h3 className="eyebrow">
                      {title} <span className="nums font-normal">{rows.length}</span>
                    </h3>
                    {rows.length === 0 && <p className="text-xs text-muted-foreground">Nothing in the index.</p>}
                    {rows.map((l) => (
                      <button
                        key={other(l)}
                        onClick={() => setPicked(other(l))}
                        className="flex flex-col gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors duration-100 hover:bg-muted"
                      >
                        <span className="flex items-center gap-2 text-sm">
                          <span className="min-w-0 flex-1 truncate font-semibold">{other(l)}</span>
                          <span className="nums text-xs text-muted-foreground">
                            {l.weight} link{l.weight === 1 ? "" : "s"}
                          </span>
                        </span>
                        {l.via.length > 0 && (
                          <span className={cn("truncate font-mono text-xs", l.cycle ? "text-warning" : "text-accent-foreground")}>
                            {l.via.slice(0, 2).join(", ")}
                            {l.via.length > 2 && <span className="font-sans text-muted-foreground"> and {l.via.length - 2} more</span>}
                          </span>
                        )}
                      </button>
                    ))}
                  </section>
                );
              })}
            </div>
          </div>
        )}
      </div>
      {alone.length > 0 && (
        <p className="flex flex-wrap items-center gap-1.5 border-t px-5 py-3 text-xs text-muted-foreground">
          <span>No links to the others:</span>
          {alone.map((r) => (
            <span key={r} className="flex items-center gap-1.5 rounded-sm bg-muted px-1.5 py-0.5 text-foreground">
              <RepoDot repo={r} /> {r}
            </span>
          ))}
        </p>
      )}
    </div>
  );
}
