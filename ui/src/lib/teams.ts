import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { useSearchParams } from "react-router";
import { api, type ContractView, type Entry, type FlowRow, type OrgMap, type Package, type TeamDef } from "@/lib/api";

/** The filter value for repositories no team labels. */
export const UNASSIGNED = "none";
const KEY = "comindly.teams";
const CONTRACTS = "_contracts";

const listeners = new Set<() => void>();
let memory: string | null = null;

function readStored(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return memory;
  }
}

function writeStored(v: string) {
  memory = v;
  try {
    localStorage.setItem(KEY, v);
  } catch {
    // storage may be unavailable
  }
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => (listeners.add(l), () => listeners.delete(l));
const split = (v: string) =>
  v
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

/** Teams of the org, yours, the selection in view and whether a repository is in it. */
export function useTeams() {
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, staleTime: Infinity });
  const [params, setParams] = useSearchParams();
  const stored = useSyncExternalStore(subscribe, readStored);
  const teams: TeamDef[] = useMemo(() => team.data?.settings.teams ?? [], [team.data]);
  const email = me.data?.mode === "server" ? me.data.email?.toLowerCase() : undefined;
  const mine = useMemo(() => (email ? teams.filter((t) => t.members.includes(email)).map((t) => t.name) : []), [teams, email]);

  // A shared link's selection is adopted, so it survives moving between pages.
  const fromUrl = params.get("teams");
  useEffect(() => {
    if (fromUrl !== null && fromUrl !== stored) writeStored(fromUrl);
  }, [fromUrl, stored]);

  const raw = fromUrl ?? stored;
  const known = (n: string) => n === UNASSIGNED || teams.some((t) => t.name.toLowerCase() === n.toLowerCase());
  const selected = useMemo(() => (raw === null ? mine : split(raw).filter(known)), [raw, mine, teams]);

  const labels = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const t of teams) for (const r of t.repos) m.set(r, [...(m.get(r) ?? []), t.name]);
    return m;
  }, [teams]);

  const inView = useCallback(
    (repo: string) => {
      if (!selected.length || repo === CONTRACTS) return true;
      const of = labels.get(repo) ?? [];
      return selected.some((s) => (s === UNASSIGNED ? of.length === 0 : of.some((l) => l.toLowerCase() === s.toLowerCase())));
    },
    [selected, labels],
  );

  const select = useCallback(
    (names: string[]) => {
      const v = names.join(",");
      writeStored(v);
      if (params.has("teams") || names.length) {
        const next = new URLSearchParams(params);
        if (names.length) next.set("teams", v);
        else next.delete("teams");
        setParams(next, { replace: true });
      }
    },
    [params, setParams],
  );

  const any = (repos: string[]) => !repos.length || repos.some(inView);
  return {
    teams,
    mine,
    selected,
    select,
    inView,
    flowInView: (f: FlowRow) => any([f.entry.repo, ...f.repos]),
    contractInView: (c: ContractView) => any([...c.providers, ...c.consumers].map((n) => n.repo)),
    entryInView: (e: Entry) => any(e.record.anchors.map((a) => a.repo)),
    packageInView: (p: Package) => inView(p.publisher) || inView(p.consumer),
    teamsOf: (repo: string) => labels.get(repo) ?? [],
    server: me.data?.mode === "server",
    version: team.data?.version ?? null,
    setUp: team.data?.settings.teams != null,
  };
}

export type Teams = ReturnType<typeof useTeams>;

/** The org map for a selection: outside repositories fold into one box per team, keeping their links to what's in view. */
export function scopeOrg(map: OrgMap | undefined, t: Teams) {
  const border = new Map<string, { team: string; repos: string[] }>();
  if (!map || !t.selected.length) return { map, border };
  const home = new Map<string, string>();
  const outside = new Map<string, string[]>();
  for (const n of map.nodes) {
    if (t.inView(n.repo)) continue;
    const team = t.teamsOf(n.repo)[0] ?? "Unassigned";
    outside.set(team, [...(outside.get(team) ?? []), n.repo]);
  }
  for (const [team, repos] of outside) {
    const id = `${team} · ${repos.length} ${repos.length === 1 ? "repo" : "repos"}`;
    border.set(id, { team, repos });
    for (const r of repos) home.set(r, id);
  }
  const nodes = map.nodes.filter((n) => !home.has(n.repo));
  for (const id of border.keys()) nodes.push({ id, name: `${id} (0)`, repo: id });
  const merged = new Map<string, OrgMap["edges"][number]>();
  for (const e of map.edges) {
    const src = home.get(e.src) ?? e.src;
    const dst = home.get(e.dst) ?? e.dst;
    if (src === dst || (home.has(e.src) && home.has(e.dst))) continue;
    const k = `${src}>${dst}>${e.kind}`;
    const m = merged.get(k);
    if (m) {
      m.weight += e.weight;
      m.via = [...new Set([...(m.via ?? []), ...(e.via ?? [])])];
    } else merged.set(k, { ...e, src, dst, via: e.via ? [...e.via] : undefined });
  }
  return { map: { nodes, edges: [...merged.values()] }, border };
}
