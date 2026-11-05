import { useQuery } from "@tanstack/react-query";
import { Bot, Check, Copy } from "lucide-react";
import { useState } from "react";
import { useCopy } from "@/components/index-log";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Toggle, ToggleGroup } from "@/components/ui/tabs";
import { api } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

export type AgentAsk = { label: string; tool: string; args?: Record<string, unknown> };

/** What an agent gets from comindly about what the page shows: the MCP tool's own answer, ready to copy. */
export function AgentView({ asks, className, icon = false }: { asks: AgentAsk[]; className?: string; icon?: boolean }) {
  const [open, setOpen] = useState(false);
  const [pick, setPick] = useState(0);
  const ask = asks[Math.min(pick, asks.length - 1)];
  const args = Object.fromEntries(Object.entries(ask?.args ?? {}).filter(([, v]) => v !== undefined && v !== ""));
  const answer = useQuery({
    queryKey: ["agent", ask?.tool, args],
    queryFn: () => api.agent(ask.tool, args),
    enabled: open && !!ask,
    staleTime: 30_000,
    retry: false,
  });
  const { copy, state, live } = useCopy();
  if (!ask) return null;
  const copied = answer.data !== undefined && state(answer.data) === "copied";
  return (
    <>
      {icon ? (
        <Button
          size="icon"
          variant="ghost"
          className={cn("size-7 shrink-0 text-muted-foreground hover:text-foreground", className)}
          onClick={() => setOpen(true)}
          aria-label="View as agent"
          title="View as agent: what comindly's MCP tools tell an agent about this"
        >
          <Bot />
        </Button>
      ) : (
        <Button size="sm" variant="outline" className={className} onClick={() => setOpen(true)} title="What comindly's MCP tools tell an agent about what this page shows">
          <Bot /> View as agent
        </Button>
      )}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="top-[8%] flex max-h-[84vh] max-w-3xl flex-col gap-3 p-5">
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <DialogTitle className="text-[17px] font-semibold">Agent context</DialogTitle>
            <span className="text-xs text-muted-foreground">
              The MCP tool <code className="font-mono text-foreground">{ask.tool}</code> answers this
            </span>
          </div>
          {asks.length > 1 && (
            <ToggleGroup label="View" value={String(pick)} onValueChange={(v) => setPick(Number(v))} className="self-start">
              {asks.map((a, i) => (
                <Toggle key={a.tool + i} value={String(i)}>
                  {a.label}
                </Toggle>
              ))}
            </ToggleGroup>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto rounded-md bg-muted">
            {answer.isLoading ? (
              <Skeleton className="m-3.5 h-40" />
            ) : answer.error ? (
              <p className="p-3.5 text-sm text-danger">{sentence(answer.error.message)}</p>
            ) : (
              <pre className="whitespace-pre-wrap p-3.5 font-mono text-xs leading-relaxed text-foreground [overflow-wrap:anywhere]">{answer.data}</pre>
            )}
          </div>
          <div className="flex items-center justify-end gap-2">
            <span className="mr-auto text-xs text-muted-foreground">Paste it into any agent, or connect the agent to comindly to ask it directly.</span>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Close
            </Button>
            <Button variant="brand" disabled={!answer.data} onClick={() => answer.data && copy(answer.data)}>
              {copied ? <Check /> : <Copy />} {copied ? "Copied" : "Copy"}
            </Button>
          </div>
          {live}
        </DialogContent>
      </Dialog>
    </>
  );
}
