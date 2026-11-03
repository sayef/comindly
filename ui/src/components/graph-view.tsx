import { Graph, layout } from "@dagrejs/dagre";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMemo, useState } from "react";
import { useTheme } from "@/components/theme";
import { CONTRACTS, cn, repoColor } from "@/lib/utils";

export interface GraphNode {
  id: string;
  label: string;
  repo: string;
}

export interface GraphEdge {
  src: string;
  dst: string;
  kind: string;
  weight?: number;
}

interface Props {
  nodes: GraphNode[];
  edges: GraphEdge[];
  focus?: string;
  /** Highlighted node ids (everything else is dimmed); `null` shows all. */
  highlight?: Set<string> | null;
  onPick?: (id: string) => void;
  className?: string;
}

type Data = { label: string; repo: string; focus: boolean; dim: boolean; more: boolean; showRepo: boolean; large: boolean };

/** Neighbours shown per side of the focus before the rest fold into a "+N more" node. */
const PER_SIDE = 12;
const HEIGHT = 44;
/** Keeps 13px labels at 12px or more on screen; larger graphs pan instead, centred on the focus on phones. */
const MIN_ZOOM = 0.93;

const color = (repo: string) => (repo === CONTRACTS ? "var(--color-brand)" : repoColor(repo));
const isContract = (kind: string) => kind === "contract" || kind === "counterpart";

function SymbolNode({ data }: NodeProps<Node<Data>>) {
  return (
    <div
      className={cn(
        "flex h-10 max-w-[280px] cursor-pointer items-center gap-2 rounded-md bg-card px-3 text-[13px] leading-tight shadow-[0_1px_2px_var(--shadow-ink)] transition-[opacity,background-color] duration-100 hover:bg-muted",
        data.large && "text-sm font-semibold",
        data.focus && "bg-accent font-semibold outline-2 outline-brand",
        data.more && "cursor-default border border-dashed border-input bg-transparent text-muted-foreground shadow-none",
        data.dim && "opacity-25",
      )}
    >
      <Handle type="target" position={data.large ? Position.Top : Position.Left} className="!pointer-events-none !opacity-0" />
      {!data.more && <span className="size-2 shrink-0 rounded-[2px]" style={{ background: color(data.repo) }} />}
      <div className="min-w-0">
        <div className={cn("truncate", data.repo === CONTRACTS && "font-mono")}>{data.label}</div>
        {data.showRepo && <div className="truncate font-normal text-muted-foreground">{data.repo}</div>}
      </div>
      <Handle type="source" position={data.large ? Position.Bottom : Position.Right} className="!pointer-events-none !opacity-0" />
    </div>
  );
}

const nodeTypes = { symbol: SymbolNode };

/** One edge per pair; a pair counts as a contract only when nothing else links it. */
function merge(edges: GraphEdge[]) {
  const by = new Map<string, GraphEdge>();
  for (const e of edges) {
    const k = `${e.src}->${e.dst}`;
    const prev = by.get(k);
    if (!prev) by.set(k, { ...e });
    else {
      prev.weight = (prev.weight ?? 1) + (e.weight ?? 1);
      if (!isContract(e.kind)) prev.kind = e.kind;
    }
  }
  return [...by.values()];
}

/** Keep the focus and up to PER_SIDE dependents and dependencies; fold the rest. */
function cap(nodes: GraphNode[], all: GraphEdge[], focus?: string) {
  const edges = merge(all);
  if (!focus || nodes.length <= PER_SIDE * 2 + 1) return { nodes, edges };
  const incoming = edges.filter((e) => e.dst === focus).map((e) => e.src);
  const outgoing = edges.filter((e) => e.src === focus).map((e) => e.dst);
  const keep = new Set([focus, ...incoming.slice(0, PER_SIDE), ...outgoing.slice(0, PER_SIDE)]);
  const kept = nodes.filter((n) => keep.has(n.id));
  const keptEdges = edges.filter((e) => keep.has(e.src) && keep.has(e.dst));
  const fold = (ids: string[], side: "in" | "out") => {
    const rest = ids.length - PER_SIDE;
    if (rest <= 0) return;
    const id = `more:${side}`;
    kept.push({ id, label: `+${rest} more ${side === "in" ? "dependents" : "dependencies"}`, repo: "" });
    keptEdges.push(side === "in" ? { src: id, dst: focus, kind: "more" } : { src: focus, dst: id, kind: "more" });
  };
  fold(incoming, "in");
  fold(outgoing, "out");
  return { nodes: kept, edges: keptEdges };
}

function place(nodes: GraphNode[], edges: GraphEdge[], wide: boolean) {
  const g = new Graph();
  g.setGraph({ rankdir: wide ? "TB" : "LR", nodesep: wide ? 48 : 14, ranksep: wide ? 50 : 110, marginx: 16, marginy: 16 });
  g.setDefaultEdgeLabel(() => ({}));
  const width = (n: GraphNode) => Math.min(280, Math.max(120, 44 + Math.max(n.label.length, n.repo.length * 0.8) * (wide ? 8 : 7.2)));
  for (const n of nodes) g.setNode(n.id, { width: width(n), height: HEIGHT });
  for (const e of edges) if (e.src !== e.dst) g.setEdge(e.src, e.dst);
  layout(g);
  const at = new Map(
    nodes.map((n) => {
      const p = g.node(n.id);
      return [n.id, { x: p.x - p.width / 2, y: p.y - HEIGHT / 2 }];
    }),
  );
  return { at, height: g.graph().height ?? 0 };
}

/** A dependency graph, dependents before dependencies: left to right around a focus, top down for the org. */
export function GraphView({ nodes, edges, focus, highlight, onPick, className }: Props) {
  const { theme } = useTheme();
  const [hovered, setHovered] = useState<string | null>(null);
  const shown = useMemo(() => cap(nodes, edges, focus), [nodes, edges, focus]);
  const { at: positions, height: laid } = useMemo(() => place(shown.nodes, shown.edges, !focus), [shown, focus]);
  const focusRepo = shown.nodes.find((n) => n.id === focus)?.repo;
  const maxW = Math.max(1, ...shown.edges.map((e) => e.weight ?? 1));
  const narrow = window.innerWidth < 640;
  const height = focus ? Math.min(narrow ? 480 : 920, Math.max(300, laid + 60)) : undefined;

  const flowNodes: Node<Data>[] = shown.nodes.map((n) => ({
    id: n.id,
    type: "symbol",
    position: positions.get(n.id) ?? { x: 0, y: 0 },
    data: {
      label: n.label,
      repo: n.repo,
      focus: n.id === focus,
      more: n.id.startsWith("more:"),
      dim: !!highlight && !highlight.has(n.id) && n.id !== focus,
      showRepo: !!n.repo && n.repo !== CONTRACTS && !!focus && n.repo !== focusRepo,
      large: !focus,
    },
  }));

  const flowEdges: Edge[] = shown.edges
    .filter((e) => e.src !== e.dst)
    .map((e) => {
      const touched = hovered === e.src || hovered === e.dst;
      const lit = !highlight || ((highlight.has(e.src) || e.src === focus) && (highlight.has(e.dst) || e.dst === focus));
      const stroke = touched ? "var(--color-foreground)" : isContract(e.kind) ? "var(--color-brand)" : "color-mix(in srgb, var(--color-muted-foreground) 70%, transparent)";
      const width = e.weight ? 1 + (Math.log(e.weight + 1) / Math.log(maxW + 1)) * 3 : 1.25;
      return {
        id: `${e.src}->${e.dst}`,
        source: e.src,
        target: e.dst,
        style: {
          stroke,
          strokeWidth: touched ? width + 0.75 : width,
          strokeDasharray: e.kind === "more" ? "4 4" : undefined,
          opacity: lit ? (hovered && !touched ? 0.25 : 0.8) : 0.08,
        },
        markerEnd: { type: MarkerType.ArrowClosed, color: stroke, width: 14, height: 14 },
        zIndex: touched ? 1 : 0,
      };
    });

  return (
    <div className={cn("relative", className ?? "h-[420px] w-full")} style={height ? { height } : undefined}>
      <ReactFlow
        key={shown.nodes.map((n) => n.id).join("|")}
        nodes={flowNodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        colorMode={theme === "dark" ? "dark" : "light"}
        fitView
        fitViewOptions={
          focus && narrow
            ? { nodes: [{ id: focus }], minZoom: MIN_ZOOM, maxZoom: MIN_ZOOM }
            : { padding: 0.12, maxZoom: 1.2, minZoom: MIN_ZOOM }
        }
        minZoom={0.15}
        zoomOnScroll={false}
        panOnScroll={false}
        preventScrolling={false}
        zoomOnDoubleClick={false}
        nodesConnectable={false}
        elementsSelectable={false}
        onNodeClick={(_, n) => !n.id.startsWith("more:") && onPick?.(n.id)}
        onNodeMouseEnter={(_, n) => setHovered(n.id)}
        onNodeMouseLeave={() => setHovered(null)}
        style={{ background: "transparent" }}
      >
        <Background variant={BackgroundVariant.Dots} gap={18} size={1} />
        <Controls showInteractive={false} position="bottom-right" />
      </ReactFlow>
    </div>
  );
}
