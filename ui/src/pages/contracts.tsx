import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { SymbolLink } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Toggle, ToggleGroup } from "@/components/ui/tabs";
import { api, type ContractView } from "@/lib/api";

const KINDS = ["all", "http", "sqs", "sns", "s3", "table"] as const;
type Kind = (typeof KINDS)[number];
type Side = "all" | "linked" | "dangling";
const LIMIT = 400;

const linked = (v: ContractView) => v.providers.length > 0 && v.consumers.length > 0;

function Participants({ nodes, empty }: { nodes: ContractView["providers"]; empty: string }) {
  const [all, setAll] = useState(false);
  if (nodes.length === 0) return <span className="text-xs text-muted-foreground">{empty}</span>;
  return (
    <div className="flex min-w-0 flex-col gap-1">
      {(all ? nodes : nodes.slice(0, 6)).map((n) => (
        <SymbolLink key={n.id} node={n} />
      ))}
      {nodes.length > 6 && (
        <button className="self-start rounded-sm text-xs font-semibold text-brand hover:underline" onClick={() => setAll(!all)} aria-expanded={all}>
          {all ? "Show fewer" : `Show ${nodes.length - 6} more`}
        </button>
      )}
    </div>
  );
}

/** The repositories on one side of a contract, for the phone row. */
function repos(nodes: ContractView["providers"], empty: string) {
  const names = [...new Set(nodes.map((n) => n.repo))];
  return names.length ? names.join(", ") : empty;
}

const phoneSelect = "h-9 w-full min-w-0 rounded-md border border-input bg-card px-2 text-sm sm:hidden";

export function ContractsPage() {
  const contracts = useQuery({ queryKey: ["contracts"], queryFn: api.contracts });
  const [params, setParams] = useSearchParams();
  const kind = (KINDS as readonly string[]).includes(params.get("kind") ?? "") ? (params.get("kind") as Kind) : "all";
  const side = (["linked", "dangling"].includes(params.get("side") ?? "") ? params.get("side") : "all") as Side;
  const q = params.get("q") ?? "";
  const set = (k: string, v: string) => setParams((p) => (v && v !== "all" ? p.set(k, v) : p.delete(k), p), { replace: true });
  const counts = useMemo(() => {
    const c: Record<string, number> = { all: contracts.data?.length ?? 0 };
    for (const v of contracts.data ?? []) {
      const k = v.contract.name.split(":")[0];
      c[k] = (c[k] ?? 0) + 1;
    }
    return c;
  }, [contracts.data]);
  const rows = useMemo(() => {
    const needle = q.toLowerCase();
    return (contracts.data ?? [])
      .filter((v) => {
        if (kind !== "all" && !v.contract.name.startsWith(`${kind}:`)) return false;
        if (side === "linked" && !linked(v)) return false;
        if (side === "dangling" && linked(v)) return false;
        if (!needle) return true;
        return [v.contract, ...v.providers, ...v.consumers].some((n) => `${n.repo}:${n.name}`.toLowerCase().includes(needle));
      })
      .sort((a, b) => Number(linked(b)) - Number(linked(a)));
  }, [contracts.data, kind, side, q]);

  const all = contracts.data ?? [];
  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Service contracts">
        Routes, queues, topics, buckets and tables, with the code that provides them and the code that uses them.
      </PageHeader>
      <div className="grid grid-cols-2 items-center gap-2 sm:flex sm:flex-wrap sm:gap-3">
        {/* Phones get two selects; the segmented groups need more width than a phone has. */}
        <select className={phoneSelect} value={kind} onChange={(e) => set("kind", e.target.value)} aria-label="Kind">
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {k === "all" ? "All kinds" : k === "table" ? "Tables" : k.toUpperCase()} ({counts[k] ?? 0})
            </option>
          ))}
        </select>
        <select className={phoneSelect} value={side} onChange={(e) => set("side", e.target.value)} aria-label="Sides">
          <option value="all">Either side</option>
          <option value="linked">Both sides</option>
          <option value="dangling">One side</option>
        </select>
        <ToggleGroup label="Kind" className="max-sm:hidden" value={kind} onValueChange={(v) => set("kind", v)}>
          {KINDS.map((k) => (
            <Toggle key={k} value={k}>
              {k === "all" ? "All" : k === "table" ? "Tables" : k.toUpperCase()}{" "}
              <span className="nums text-xs font-normal text-muted-foreground">{counts[k] ?? 0}</span>
            </Toggle>
          ))}
        </ToggleGroup>
        <ToggleGroup label="Sides" className="max-sm:hidden" value={side} onValueChange={(v) => set("side", v)}>
          <Toggle value="all">Either side</Toggle>
          <Toggle value="linked">Both sides</Toggle>
          <Toggle value="dangling">One side</Toggle>
        </ToggleGroup>
        <div className="relative col-span-2 w-full sm:ml-auto sm:max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input
            value={q}
            onChange={(e) => set("q", e.target.value)}
            placeholder="Filter by name or repository"
            aria-label="Filter contracts"
            spellCheck={false}
            className="pl-8"
          />
        </div>
      </div>
      <Card>
        {contracts.isLoading ? (
          <div className="flex flex-col gap-2 p-4">
            {[0, 1, 2, 3, 4].map((i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : (
          <Table reflow className="sm:min-w-[720px] sm:table-fixed">
            <thead>
              <tr className="border-b">
                <Th className="w-[36%]">Contract</Th>
                <Th className="w-[32%]">Provided by</Th>
                <Th className="w-[32%]">Used by</Th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, LIMIT).map((v) => {
                const k = v.contract.name.split(":")[0];
                return (
                  <Tr key={v.contract.id}>
                    <Td slot="main">
                      <div className="flex min-w-0 items-center gap-2">
                        <Badge variant="outline" className="w-12 shrink-0 justify-center">
                          {k === "table" ? "Table" : k.toUpperCase()}
                        </Badge>
                        <SymbolLink node={{ ...v.contract, name: v.contract.name.split(":").slice(1).join(":") }} />
                      </div>
                    </Td>
                    <Td slot="desktop">
                      <Participants nodes={v.providers} empty="not in the index" />
                    </Td>
                    <Td slot="desktop">
                      <Participants nodes={v.consumers} empty="no caller indexed" />
                    </Td>
                    <Td slot="meta" className="hidden max-sm:flex max-sm:items-center max-sm:gap-1.5">
                      <span className="max-w-[60%] shrink-0 truncate">{repos(v.providers, "not in the index")}</span>
                      <ArrowRight className="size-3 shrink-0" aria-label="used by" />
                      <span className="min-w-0 truncate">{repos(v.consumers, "no caller indexed")}</span>
                    </Td>
                  </Tr>
                );
              })}
            </tbody>
          </Table>
        )}
        {!contracts.isLoading && rows.length === 0 && (
          <p className="px-5 py-8 text-muted-foreground">
            {all.length ? "No contract matches these filters. Clear the filter text or pick All." : "No contracts yet. They appear once repositories with routes, queues or tables are indexed."}
          </p>
        )}
      </Card>
      <p className="nums text-xs text-muted-foreground">
        Showing {Math.min(rows.length, LIMIT)} of {rows.length} {rows.length === 1 ? "contract" : "contracts"}
        {rows.length < all.length && ` (${all.length} in the index)`}
      </p>
    </div>
  );
}
