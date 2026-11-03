import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useSearchParams } from "react-router";
import { RepoDot } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Toggle, ToggleGroup } from "@/components/ui/tabs";
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
  if (!p.skewed || !p.locked || !p.published) return <span className="font-mono text-xs">{p.locked ?? "not pinned"}</span>;
  const behind = compare(p.locked, p.published) < 0;
  return (
    <span className="flex items-center gap-2 text-xs">
      <span className={cn("font-mono", behind && "text-warning")}>{p.locked}</span>
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
  const [params, setParams] = useSearchParams();
  const only: "all" | "skew" = params.get("only") === "skew" ? "skew" : "all";
  const setOnly = (v: string) => setParams((p) => (v === "skew" ? p.set("only", v) : p.delete("only"), p), { replace: true });
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
      <PageHeader title="Internal packages">
        Libraries one repository publishes and others depend on, with the version each consumer pins. A pin that differs from the
        published version is skew: the consumer runs code the publisher has moved on from.
      </PageHeader>
      <ToggleGroup label="Show" value={only} onValueChange={setOnly} className="self-start">
        <Toggle value="all">All</Toggle>
        <Toggle value="skew">
          Version skew <span className="nums text-xs font-normal text-muted-foreground">{skewed}</span>
        </Toggle>
      </ToggleGroup>
      {rows.isLoading && <Skeleton className="h-48 rounded-lg" />}
      {!rows.isLoading && groups.length === 0 && (
        <p className="rounded-lg border border-dashed p-10 text-center text-sm text-muted-foreground">
          {only === "skew" ? "Every consumer pins its publisher's version." : "No internal packages found in the indexed manifests."}
        </p>
      )}
      {groups.map((g) => (
        <Card key={`${g[0].ecosystem}:${g[0].name}`} className="overflow-hidden">
          <div className="flex flex-wrap items-center gap-2 border-b px-5 py-3.5">
            <h2 className="font-mono font-semibold [overflow-wrap:anywhere]">{g[0].name}</h2>
            <Badge variant="outline">{g[0].ecosystem}</Badge>
            <span className="text-muted-foreground">published by</span>
            <span className="flex items-center gap-1.5">
              <RepoDot repo={g[0].publisher} /> {g[0].publisher}
            </span>
            {g[0].published && <span className="nums text-muted-foreground">at version <span className="font-mono">{g[0].published}</span></span>}
          </div>
          <Table reflow>
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
                    <Td slot="main">
                      <span className="flex min-w-0 items-center gap-1.5">
                        <RepoDot repo={p.consumer} /> <span className="truncate">{p.consumer}</span>
                      </span>
                    </Td>
                    <Td slot="meta" className="font-mono text-xs">
                      <span className="hidden font-sans max-sm:inline">Requires </span>
                      {p.requirement ?? <span className="font-sans text-muted-foreground">any version</span>}
                    </Td>
                    <Td slot="end">
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
