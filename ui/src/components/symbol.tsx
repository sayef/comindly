import { Link } from "react-router";
import type { Node } from "@/lib/api";
import { CONTRACTS, cn, repoHue } from "@/lib/utils";

export function RepoDot({ repo, className }: { repo: string; className?: string }) {
  return (
    <span
      className={cn("inline-block size-2 shrink-0 rounded-[2px]", className)}
      style={{ background: repo === CONTRACTS ? "var(--color-brand)" : `hsl(${repoHue(repo)} 62% 62%)` }}
    />
  );
}

export function SymbolLink({ node, showRepo = true }: { node: Node; showRepo?: boolean }) {
  const contract = node.repo === CONTRACTS;
  return (
    <Link
      to={`/explorer?focus=${encodeURIComponent(node.id)}`}
      title={contract ? node.name : `${node.repo}, ${node.location}`}
      className="group inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-sm text-sm hover:text-brand"
    >
      <RepoDot repo={node.repo} />
      <span className={cn("min-w-0 truncate", contract && "font-mono text-xs")}>
        {showRepo && !contract && <span className="text-muted-foreground group-hover:text-brand/80">{node.repo} </span>}
        {node.name}
      </span>
    </Link>
  );
}
