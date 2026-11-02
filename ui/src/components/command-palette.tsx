import { useQuery } from "@tanstack/react-query";
import { Command } from "cmdk";
import { FileCode2, FunctionSquare, Network, Shapes, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { CONTRACTS, sentence } from "@/lib/utils";

function iconFor(n: { repo: string; kind: string }) {
  if (n.repo === CONTRACTS) return Network;
  if (n.kind === "File") return FileCode2;
  if (["Class", "Struct", "Interface", "Trait", "Enum"].includes(n.kind)) return Shapes;
  return FunctionSquare;
}

const heading = "px-3 pb-1 pt-2 text-xs font-semibold text-muted-foreground";
const item = "flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm data-[selected=true]:bg-muted";

export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  const navigate = useNavigate();
  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 200);
    return () => clearTimeout(t);
  }, [q]);
  const names = useQuery({ queryKey: ["find", debounced], queryFn: () => api.find(debounced), enabled: debounced.length > 1 });
  const code = useQuery({
    queryKey: ["search", debounced],
    queryFn: () => api.search(debounced),
    enabled: debounced.length > 2,
    retry: false,
  });
  const go = (id: string) => {
    onOpenChange(false);
    navigate(`/explorer?focus=${encodeURIComponent(id)}`);
  };
  const shownIds = new Set((names.data ?? []).slice(0, 8).map((n) => n.id));
  const hits = (code.data ?? []).filter((h) => !shownIds.has(h.id)).slice(0, 10);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogTitle className="sr-only">Search the index</DialogTitle>
        <Command shouldFilter={false} className="flex flex-col">
          <Command.Input
            value={q}
            onValueChange={setQ}
            placeholder="Search by name, or describe what you are looking for…"
            className="h-12 border-b bg-transparent px-4 text-sm outline-none placeholder:text-muted-foreground max-sm:text-base"
          />
          <Command.List className="max-h-[65vh] overflow-y-auto p-2">
            {debounced.length <= 1 && (
              <div className="px-3 py-8 text-center text-sm text-muted-foreground">
                Type a symbol, file or route, or a question like “where do we retry HTTP calls”.
              </div>
            )}
            {debounced.length > 1 && !names.isFetching && !code.isFetching && (
              <Command.Empty className="px-3 py-8 text-center text-sm text-muted-foreground">Nothing in the index matches. Try part of a name or path.</Command.Empty>
            )}
            {(names.data?.length ?? 0) > 0 && (
              <Command.Group>
                <div className={heading}>By name</div>
                {names.data!.slice(0, 8).map((n) => {
                  const Icon = iconFor(n);
                  return (
                    <Command.Item key={n.id} value={`name:${n.id}`} onSelect={() => go(n.id)} className={item}>
                      <Icon className="size-4 shrink-0 text-muted-foreground" />
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-mono font-semibold">{n.name}</div>
                        <div className="truncate font-mono text-xs text-muted-foreground">
                          {n.repo === CONTRACTS ? "Contract" : `${n.repo}/${n.location}`}
                        </div>
                      </div>
                      <span className="text-xs text-muted-foreground">{n.kind.charAt(0).toUpperCase() + n.kind.slice(1)}</span>
                    </Command.Item>
                  );
                })}
              </Command.Group>
            )}
            {debounced.length > 2 && (
              <Command.Group>
                <div className={heading}>
                  <Sparkles className="mr-1 inline size-3" /> Code search
                </div>
                {code.isFetching && <div className="px-3 py-2 text-xs text-muted-foreground">Searching…</div>}
                {code.error && <div className="px-3 py-2 text-xs text-muted-foreground [overflow-wrap:anywhere]">{sentence(code.error.message)}</div>}
                {hits.map((h) => {
                  const Icon = iconFor(h);
                  return (
                    <Command.Item key={h.id} value={`code:${h.id}`} onSelect={() => go(h.id)} className={item}>
                      <Icon className="size-4 shrink-0 text-muted-foreground" />
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-medium">{h.name}</div>
                        <div className="truncate text-xs text-muted-foreground">
                          {h.summary ?? <span className="font-mono">{`${h.repo}/${h.location}`}</span>}
                        </div>
                      </div>
                      <span className="shrink-0 text-xs text-muted-foreground">{h.repo}</span>
                    </Command.Item>
                  );
                })}
              </Command.Group>
            )}
          </Command.List>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
