import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { NavLink, Outlet } from "react-router";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Notice } from "@/components/notice";
import { ApiKey, TeamSection } from "@/components/team-settings";
import { api, type SettingsChange } from "@/lib/api";

const field = "flex flex-col gap-1 text-xs font-medium";

function useSave() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: SettingsChange) => api.saveSettings(c),
    meta: { inline: true },
    onSuccess: () => qc.invalidateQueries(),
  });
}

export function Section({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description && <CardDescription>{description}</CardDescription>}
      </CardHeader>
      <CardContent className="flex flex-col gap-3">{children}</CardContent>
    </Card>
  );
}

export function SettingsPage() {
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings });
  const s = settings.data;
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const server = me.data?.mode === "server";
  const saveIndex = useSave();
  const saveYou = useSave();
  const saveFresh = useSave();
  const test = useMutation({ mutationFn: (dir: string) => api.testIndex(dir), meta: { inline: true, quiet: true } });

  const [index, setIndex] = useState("");
  const [name, setName] = useState("");
  const [names, setNames] = useState(true);
  const [ttl, setTtl] = useState(300);
  useEffect(() => {
    if (!s) return;
    setIndex(s.index_dir ?? s.index_effective);
    setName(s.author_name ?? "");
    setNames(s.record_names);
    setTtl(s.cache_ttl_secs);
  }, [s]);

  if (settings.isLoading || !s) return <Skeleton className="h-96 rounded-xl" />;
  const result = (m: typeof saveIndex) => (m.error ? <Notice ok={false} message={m.error.message} /> : null);

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <div>
        <h1 className="text-[28px] font-semibold tracking-[-0.035em]">Settings</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          {server ? (
            <>
              The index is <span className="font-mono">{s.index_effective}</span>, set by the deployment.
            </>
          ) : (
            <>
              Saved in <span className="font-mono">{s.config_path}</span>, shared with the command line, the MCP server and hooks.
            </>
          )}
        </p>
      </div>

      {!server && (
        <Section
          title="Index"
          description="Where the index lives: a folder, or a bucket location (s3://, gs:// or az://). Bucket access uses the credentials this machine or container already has."
        >
          <label className={field}>
            Location
            <Input value={index} onChange={(e) => setIndex(e.target.value)} placeholder="~/comindly-index or s3://bucket/comindly" />
          </label>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" disabled={!index.trim()} loading={test.isPending} onClick={() => test.mutate(index)}>
              Test
            </Button>
            <Button
              variant="brand"
              size="sm"
              disabled={!index.trim()}
              loading={saveIndex.isPending}
              onClick={() => saveIndex.mutate({ index_dir: index })}
            >
              Use this index
            </Button>
          </div>
          {test.error && <Notice ok={false} message={test.error.message} />}
          {test.data && <Notice ok message={test.data.message} />}
          {result(saveIndex)}
        </Section>
      )}

      <TeamSection />

      {!server && (
        <Section title="You" description="Shown on the ledger records you write and review.">
          <label className={field}>
            Name
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={s.author_effective ?? "from git user.name"} />
          </label>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={names} onChange={(e) => setNames(e.target.checked)} />
            Put my name on new records (off: only a pseudonymous id)
          </label>
          <div>
            <Button
              variant="outline"
              size="sm"
              loading={saveYou.isPending}
              onClick={() => saveYou.mutate({ author_name: name, record_names: names })}
            >
              Save
            </Button>
          </div>
          {result(saveYou)}
        </Section>
      )}

      {!server && (
        <Section title="Your LLM key" description="Used when indexing with LLM summaries; the endpoint and model are the index's settings above. Kept on this machine only.">
          <ApiKey />
        </Section>
      )}

      {!server && (
        <Section title="Freshness" description="How often the app, the MCP server and hooks check for a newer index version.">
          <label className={field}>
            Seconds between checks
            <Input type="number" min={30} className="max-w-40" value={ttl} onChange={(e) => setTtl(Number(e.target.value))} />
          </label>
          <div>
            <Button variant="outline" size="sm" loading={saveFresh.isPending} onClick={() => saveFresh.mutate({ cache_ttl_secs: ttl })}>
              Save
            </Button>
          </div>
          {result(saveFresh)}
        </Section>
      )}
    </div>
  );
}

/** Settings with its tabs: general settings and the repositories this machine indexes. */
export function SettingsShell() {
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const server = me.data?.mode === "server";
  const admin = me.data?.role === "admin";
  const tab = ({ isActive }: { isActive: boolean }) =>
    `rounded-md px-3 py-1.5 text-[13px] font-medium transition-colors ${isActive ? "bg-muted text-foreground" : "text-muted-foreground hover:text-foreground"}`;
  return (
    <div className="flex flex-col gap-5">
      <nav className="flex w-fit flex-wrap gap-0.5 rounded-lg border bg-card p-0.5">
        <NavLink to="/settings" end className={tab}>
          General
        </NavLink>
        <NavLink to="/settings/repositories" className={tab}>
          Repositories
        </NavLink>
        {server && admin && (
          <>
            <NavLink to="/settings/members" className={tab}>
              Members
            </NavLink>
            <NavLink to="/settings/integrations" className={tab}>
              Integrations
            </NavLink>
            <NavLink to="/settings/audit" className={tab}>
              Audit
            </NavLink>
          </>
        )}
        {server && (
          <NavLink to="/settings/access" className={tab}>
            Your access
          </NavLink>
        )}
      </nav>
      <Outlet />
    </div>
  );
}
