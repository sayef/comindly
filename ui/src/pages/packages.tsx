import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { RepoDot } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type Package } from "@/lib/api";
import { cn } from "@/lib/utils";

/** Numeric comparison of dotted versions: negative when `a` is older. */
function compare(a: string, b: string) {
  const pa = a.split(/[.+-]/).map((x) => parseInt(x, 10) || 0);
  const pb = b.split(/[.+-]/).map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) - (pb[i] ?? 0);
  }
  return 0;
}

function Pin({ p }: { p: Package }) {
  if (!p.skewed || !p.locked || !p.published) return <span className="font-mono text-xs">{p.locked ?? "—"}</span>;
  const behind = compare(p.locked, p.published) < 0;
  return (
    <span className={cn("flex items-center gap-2 font-mono text-xs", behind && "text-warning")}>
      {p.locked}
      {behind ? (
        <Badge variant="warning" title={`The publisher's source is at ${p.published}`}>
          Behind
        </Badge>
      ) : (
        <Badge variant="outline" title={`Newer than the version in the publisher's source (${p.published}); it is probably set at release time`}>
          Ahead of source
        </Badge>
      )}
    </span>
  );
}

export function PackagesPage() {
  const rows = useQuery({ queryKey: ["packages"], queryFn: api.packages });
  const [only, setOnly] = useState<"all" | "skew">("all");
  const groups = useMemo(() => {
    const m = new Map<string, Package[]>();
    for (const p of rows.data ?? []) m.set(`${p.ecosystem}:${p.name}`, [...(m.get(`${p.ecosystem}:${p.name}`) ?? []), p]);
    return [...m.values()]
      .filter((g) => only === "all" || g.some((p) => p.skewed))
      .sort((a, b) => a[0].name.localeCompare(b[0].name));
  }, [rows.data, only]);
  const skewed = (rows.data ?? []).filter((p) => p.skewed).length;

  return (
    <div className="flex flex-col gap-5">
      <div>
        <h1 className="text-[28px] font-semibold tracking-[-0.035em]">Internal packages</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Libraries one repository publishes and others depend on, with the version each consumer pins. A pin that differs from the
          published version is skew: the consumer runs code the publisher has moved on from.
        </p>
      </div>
      <Tabs value={only} onValueChange={(v) => setOnly(v as "all" | "skew")}>
        <TabsList>
          <TabsTrigger value="all">All</TabsTrigger>
          <TabsTrigger value="skew">
            Version skew <span className="font-mono text-[10px] text-muted-foreground">{skewed}</span>
          </TabsTrigger>
        </TabsList>
      </Tabs>
      {rows.isLoading && <Skeleton className="h-48 rounded-xl" />}
      {!rows.isLoading && groups.length === 0 && (
        <p className="text-sm text-muted-foreground">
          {only === "skew" ? "Every consumer pins its publisher's version." : "No internal packages found in the indexed manifests."}
        </p>
      )}
      {groups.map((g) => (
        <Card key={`${g[0].ecosystem}:${g[0].name}`} className="overflow-hidden">
          <div className="flex flex-wrap items-center gap-2 border-b px-4 py-3">
            <span className="font-mono text-sm font-semibold">{g[0].name}</span>
            <Badge variant="outline">{g[0].ecosystem}</Badge>
            <span className="text-xs text-muted-foreground">published by</span>
            <span className="flex items-center gap-1.5 text-sm">
              <RepoDot repo={g[0].publisher} /> {g[0].publisher}
            </span>
            {g[0].published && <span className="font-mono text-xs text-muted-foreground">at {g[0].published}</span>}
          </div>
          <Table>
            <thead>
              <tr className="border-b">
                <Th>Consumer</Th>
                <Th>Requires</Th>
                <Th>Pinned</Th>
              </tr>
            </thead>
            <tbody>
              {g
                .filter((p) => only === "all" || p.skewed)
                .map((p) => (
                  <Tr key={p.consumer}>
                    <Td>
                      <span className="flex items-center gap-1.5">
                        <RepoDot repo={p.consumer} /> {p.consumer}
                      </span>
                    </Td>
                    <Td className="font-mono text-xs">{p.requirement ?? "—"}</Td>
                    <Td>
                      <Pin p={p} />
                    </Td>
                  </Tr>
                ))}
            </tbody>
          </Table>
        </Card>
      ))}
    </div>
  );
}
