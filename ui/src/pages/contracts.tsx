import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { useMemo, useState } from "react";
import { SymbolLink } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type ContractView } from "@/lib/api";

const KINDS = ["all", "http", "sqs", "sns", "s3", "table"] as const;
type Side = "all" | "linked" | "dangling";

function Participants({ nodes, empty }: { nodes: ContractView["providers"]; empty: string }) {
  if (nodes.length === 0) return <span className="text-xs text-muted-foreground">{empty}</span>;
  return (
    <div className="flex min-w-0 flex-col gap-1">
      {nodes.slice(0, 6).map((n) => (
        <SymbolLink key={n.id} node={n} />
      ))}
      {nodes.length > 6 && <span className="text-xs text-muted-foreground">and {nodes.length - 6} more</span>}
    </div>
  );
}

export function ContractsPage() {
  const contracts = useQuery({ queryKey: ["contracts"], queryFn: api.contracts });
  const [kind, setKind] = useState<(typeof KINDS)[number]>("all");
  const [side, setSide] = useState<Side>("all");
  const [q, setQ] = useState("");
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
    return (contracts.data ?? []).filter((v) => {
      if (kind !== "all" && !v.contract.name.startsWith(`${kind}:`)) return false;
      const linked = v.providers.length > 0 && v.consumers.length > 0;
      if (side === "linked" && !linked) return false;
      if (side === "dangling" && linked) return false;
      if (!needle) return true;
      return [v.contract, ...v.providers, ...v.consumers].some((n) => `${n.repo}:${n.name}`.toLowerCase().includes(needle));
    });
  }, [contracts.data, kind, side, q]);

  const all = contracts.data ?? [];
  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Service contracts">
        Routes, queues, topics, buckets and tables, with the code that provides them and the code that uses them.
      </PageHeader>
      <div className="flex flex-wrap items-center gap-3">
        <Tabs value={kind} onValueChange={(v) => setKind(v as (typeof KINDS)[number])}>
          <TabsList>
            {KINDS.map((k) => (
              <TabsTrigger key={k} value={k}>
                {k === "all" ? "All" : k === "table" ? "Tables" : k.toUpperCase()}{" "}
                <span className="nums text-xs font-normal text-muted-foreground">{counts[k] ?? 0}</span>
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <Tabs value={side} onValueChange={(v) => setSide(v as Side)}>
          <TabsList>
            <TabsTrigger value="all">All</TabsTrigger>
            <TabsTrigger value="linked">Both sides</TabsTrigger>
            <TabsTrigger value="dangling">One side</TabsTrigger>
          </TabsList>
        </Tabs>
        <div className="relative ml-auto w-full max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter by name or repository" className="pl-8" />
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
          <Table className="min-w-[720px] table-fixed">
            <thead>
              <tr className="border-b">
                <Th className="w-[36%]">Contract</Th>
                <Th className="w-[32%]">Provided by</Th>
                <Th className="w-[32%]">Used by</Th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, 400).map((v) => (
                <Tr key={v.contract.id}>
                  <Td>
                    <div className="flex min-w-0 items-center gap-2">
                      <Badge variant="outline" className="w-12 justify-center">
                        {v.contract.name.split(":")[0] === "table" ? "Table" : v.contract.name.split(":")[0].toUpperCase()}
                      </Badge>
                      <SymbolLink node={{ ...v.contract, name: v.contract.name.split(":").slice(1).join(":") }} />
                    </div>
                  </Td>
                  <Td>
                    <Participants nodes={v.providers} empty="not in the index" />
                  </Td>
                  <Td>
                    <Participants nodes={v.consumers} empty="no caller indexed" />
                  </Td>
                </Tr>
              ))}
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
        Showing {Math.min(rows.length, 400)} of {all.length} contracts
      </p>
    </div>
  );
}
