import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type SimulationNodeDatum } from "d3-force";
import { Maximize, Minus, Plus, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Org } from "@/components/org-views";
import { RepoDot } from "@/components/symbol";
import { Button } from "@/components/ui/button";
import { cn, repoHue } from "@/lib/utils";

const HEIGHT = 540;

interface N extends SimulationNodeDatum {
  id: string;
  r: number;
  symbols: number;
}

interface L {
  source: N;
  target: N;
  weight: number;
  contract: boolean;
  cycle: boolean;
  via: string[];
}

const short = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));

/** Repositories as a force-directed network: size is symbols, lines are dependencies, thicker for more links. */
export function OrgNetwork({ org }: { org: Org }) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);
  const [, setTick] = useState(0);
  const [view, setView] = useState({ k: 1, x: 0, y: 0 });
  const [hover, setHover] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const drag = useRef<{ node?: N; pan?: { x: number; y: number; vx: number; vy: number }; moved?: boolean } | null>(null);

  const { nodes, links } = useMemo(() => {
    const most = Math.max(1, ...org.repos.map((r) => org.symbols.get(r) ?? 0));
    const nodes: N[] = org.repos.map((id) => {
      const symbols = org.symbols.get(id) ?? 0;
      return { id, symbols, r: 12 + 26 * Math.sqrt(symbols / most) };
    });
    const by = new Map(nodes.map((n) => [n.id, n]));
    const inCycle = (a: string, b: string) => org.cycles.some((c) => c.includes(a) && c.includes(b));
    const links: L[] = [];
    for (const [src, out] of org.uses) {
      for (const [dst, es] of out) {
        if (src === dst || !by.has(src) || !by.has(dst)) continue;
        links.push({
          source: by.get(src)!,
          target: by.get(dst)!,
          weight: es.reduce((n, e) => n + (e.weight || 1), 0),
          contract: es.some((e) => e.kind === "contract"),
          cycle: inCycle(src, dst),
          via: [...new Set(es.flatMap((e) => e.via ?? []))],
        });
      }
    }
    return { nodes, links };
  }, [org]);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setWidth(Math.max(320, e.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const sim = useMemo(
    () =>
      forceSimulation<N>(nodes)
        .force(
          "link",
          forceLink<N, L>(links)
            .distance((l) => 70 + l.source.r + l.target.r)
            .strength(0.35),
        )
        .force("charge", forceManyBody<N>().strength((n) => -260 - n.r * 12))
        .force("collide", forceCollide<N>((n) => n.r + 26))
        .force("x", forceX<N>(0).strength(0.06))
        .force("y", forceY<N>(0).strength(0.09))
        .stop(),
    [nodes, links],
  );

  // One settling animation from a calm start; with reduced motion it settles before the first paint.
  useEffect(() => {
    const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (still) {
      sim.tick(300);
      setTick((t) => t + 1);
      return;
    }
    sim.alpha(1).restart();
    sim.on("tick", () => setTick((t) => t + 1));
    return () => {
      sim.stop();
      sim.on("tick", null);
    };
  }, [sim]);

  const fit = () => {
    if (!nodes.length) return;
    const xs = nodes.flatMap((n) => [(n.x ?? 0) - n.r - 40, (n.x ?? 0) + n.r + 40]);
    const ys = nodes.flatMap((n) => [(n.y ?? 0) - n.r - 20, (n.y ?? 0) + n.r + 40]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const k = Math.min(1.6, (width - 32) / (x1 - x0), (HEIGHT - 32) / (y1 - y0));
    setView({ k, x: width / 2 - ((x0 + x1) / 2) * k, y: HEIGHT / 2 - ((y0 + y1) / 2) * k });
  };
  // Fit once the layout has mostly settled, and again when the width changes.
  useEffect(() => {
    const t = setTimeout(fit, 900);
    return () => clearTimeout(t);
  }, [sim, width]);

  const zoom = (f: number) =>
    setView((v) => {
      const k = Math.min(3, Math.max(0.3, v.k * f));
      const cx = width / 2;
      const cy = HEIGHT / 2;
      return { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k };
    });

  const point = (e: React.PointerEvent) => {
    const r = box.current!.getBoundingClientRect();
    return { x: (e.clientX - r.left - view.x) / view.k, y: (e.clientY - r.top - view.y) / view.k };
  };

  const focus = hover ?? picked;
  const near = useMemo(() => {
    if (!focus) return null;
    const s = new Set([focus]);
    for (const l of links) {
      if (l.source.id === focus) s.add(l.target.id);
      if (l.target.id === focus) s.add(l.source.id);
    }
    return s;
  }, [focus, links]);

  const uses = picked ? links.filter((l) => l.source.id === picked) : [];
  const usedBy = picked ? links.filter((l) => l.target.id === picked) : [];

  return (
    <div
      ref={box}
      className="relative touch-none overflow-hidden select-none"
      style={{ height: HEIGHT }}
      onWheel={(e) => {
        if (!e.ctrlKey && !e.metaKey) return;
        e.preventDefault();
        zoom(e.deltaY < 0 ? 1.1 : 1 / 1.1);
      }}
      onPointerDown={(e) => {
        if ((e.target as Element).closest("[data-node],[data-panel],[data-controls]")) return;
        drag.current = { pan: { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y } };
        (e.currentTarget as Element).setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        if (d.pan) setView((v) => ({ ...v, x: d.pan!.vx + e.clientX - d.pan!.x, y: d.pan!.vy + e.clientY - d.pan!.y }));
        if (d.node) {
          const p = point(e);
          d.node.fx = p.x;
          d.node.fy = p.y;
          d.moved = true;
          sim.alphaTarget(0.25).restart();
        }
      }}
      onPointerUp={() => {
        const d = drag.current;
        if (d?.node) {
          sim.alphaTarget(0);
          if (!d.moved) setPicked((p) => (p === d.node!.id ? null : d.node!.id));
          d.node.fx = null;
          d.node.fy = null;
        }
        drag.current = null;
      }}
    >
      <svg width={width} height={HEIGHT} className="block" role="img" aria-label="Repositories and the dependencies between them">
        <defs>
          {(["code", "contract", "cycle"] as const).map((k) => (
            <marker key={k} id={`tip-${k}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="9" markerHeight="9" markerUnits="userSpaceOnUse" orient="auto">
              <path d="M0,0 L10,5 L0,10 z" fill={k === "contract" ? "var(--color-brand)" : k === "cycle" ? "var(--color-warning)" : "var(--color-muted-foreground)"} />
            </marker>
          ))}
        </defs>
        <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
          {links.map((l) => {
            const { source: s, target: t } = l;
            const dx = (t.x ?? 0) - (s.x ?? 0);
            const dy = (t.y ?? 0) - (s.y ?? 0);
            const len = Math.hypot(dx, dy) || 1;
            // A slight bend, so links both ways between two repositories stay apart.
            const mx = ((s.x ?? 0) + (t.x ?? 0)) / 2 - (dy / len) * 18;
            const my = ((s.y ?? 0) + (t.y ?? 0)) / 2 + (dx / len) * 18;
            // Start and end on the circles' edges, not their centres.
            const toward = (n: N, d: number) => {
              const k = d / (Math.hypot(mx - (n.x ?? 0), my - (n.y ?? 0)) || 1);
              return [(n.x ?? 0) + (mx - (n.x ?? 0)) * k, (n.y ?? 0) + (my - (n.y ?? 0)) * k];
            };
            const [sx, sy] = toward(s, s.r + 2);
            const [ex, ey] = toward(t, t.r + 4);
            const kind = l.cycle ? "cycle" : l.contract ? "contract" : "code";
            const lit = focus && (s.id === focus || t.id === focus);
            const dim = focus && !lit;
            return (
              <path
                key={`${s.id}>${t.id}`}
                d={`M${sx},${sy} Q${mx},${my} ${ex},${ey}`}
                fill="none"
                stroke={kind === "contract" ? "var(--color-brand)" : kind === "cycle" ? "var(--color-warning)" : "var(--color-muted-foreground)"}
                strokeWidth={Math.min(6, 1 + Math.log2(l.weight + 1) * 0.7) / Math.sqrt(view.k)}
                strokeOpacity={dim ? 0.07 : lit ? 0.95 : kind === "code" ? 0.35 : 0.6}
                strokeDasharray={kind === "cycle" ? "5 4" : undefined}
                markerEnd={`url(#tip-${kind})`}
                className="transition-[stroke-opacity] duration-150"
              >
                <title>{`${s.id} uses ${t.id}: ${l.weight} link${l.weight === 1 ? "" : "s"}${l.via.length ? `, through ${l.via.slice(0, 4).join(", ")}` : ""}`}</title>
              </path>
            );
          })}
          {nodes.map((n) => {
            const hue = repoHue(n.id);
            const dim = near && !near.has(n.id);
            const on = n.id === picked || n.id === hover;
            return (
              <g
                key={n.id}
                data-node
                tabIndex={0}
                role="button"
                aria-label={`${n.id}, ${n.symbols.toLocaleString()} symbols`}
                aria-pressed={n.id === picked}
                transform={`translate(${n.x ?? 0},${n.y ?? 0})`}
                className={cn("cursor-pointer outline-none transition-opacity duration-150", dim && "opacity-20")}
                onPointerEnter={() => setHover(n.id)}
                onPointerLeave={() => setHover(null)}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  drag.current = { node: n, moved: false };
                  box.current!.setPointerCapture(e.pointerId);
                }}
                onFocus={() => setHover(n.id)}
                onBlur={() => setHover(null)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setPicked((p) => (p === n.id ? null : n.id));
                  }
                  if (e.key === "Escape") setPicked(null);
                }}
              >
                <circle r={n.r + 6} fill="transparent" />
                <circle
                  r={n.r}
                  fill={`color-mix(in srgb, hsl(${hue} 62% 58%) ${on ? 34 : 20}%, var(--color-card))`}
                  stroke={`hsl(${hue} 58% 52%)`}
                  strokeWidth={(on ? 2.5 : 1.5) / Math.sqrt(view.k)}
                  className="transition-[fill] duration-150"
                />
                <text y={n.r + 15} textAnchor="middle" className="fill-foreground text-xs font-semibold" style={{ paintOrder: "stroke", stroke: "var(--color-card)", strokeWidth: 4, strokeLinejoin: "round" }}>
                  {n.id}
                </text>
                <text y={4} textAnchor="middle" className="nums fill-muted-foreground text-xs" style={{ display: n.r > 20 ? undefined : "none" }}>
                  {short(n.symbols)}
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

      {picked && (
        <div data-panel className="absolute top-3 right-3 bottom-3 flex w-80 max-w-[calc(100%-1.5rem)] flex-col overflow-hidden rounded-lg border bg-popover shadow-float">
          <div className="flex items-center gap-2 border-b px-4 py-3">
            <RepoDot repo={picked} />
            <span className="min-w-0 flex-1 truncate font-semibold">{picked}</span>
            <span className="nums text-xs text-muted-foreground">{(org.symbols.get(picked) ?? 0).toLocaleString()} symbols</span>
            <Button variant="ghost" size="icon" className="size-7" aria-label="Close" onClick={() => setPicked(null)}>
              <X />
            </Button>
          </div>
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto overscroll-contain p-4">
            {[
              ["Uses", uses, (l: L) => l.target.id],
              ["Used by", usedBy, (l: L) => l.source.id],
            ].map(([title, list, other]) => {
              const rows = (list as L[]).slice().sort((a, b) => b.weight - a.weight);
              const name = other as (l: L) => string;
              return (
                <section key={title as string} className="flex flex-col gap-1.5">
                  <h3 className="eyebrow">
                    {title as string} <span className="nums font-normal">{rows.length}</span>
                  </h3>
                  {rows.length === 0 && <p className="text-xs text-muted-foreground">Nothing in the index.</p>}
                  {rows.map((l) => (
                    <button
                      key={name(l)}
                      onClick={() => setPicked(name(l))}
                      className="flex flex-col gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors duration-100 hover:bg-muted"
                    >
                      <span className="flex items-center gap-2 text-sm">
                        <RepoDot repo={name(l)} />
                        <span className="min-w-0 flex-1 truncate font-semibold">{name(l)}</span>
                        <span className="nums text-xs text-muted-foreground">
                          {l.weight} link{l.weight === 1 ? "" : "s"}
                        </span>
                      </span>
                      {l.via.length > 0 && (
                        <span className={cn("truncate pl-4 font-mono text-xs", l.cycle ? "text-warning" : "text-accent-foreground")}>
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
  );
}
