import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, useContext, useLayoutEffect, useState } from "react";
import { NavLink, Outlet } from "react-router";
import { PageHeader } from "@/components/page-header";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Notice } from "@/components/notice";
import { ApiKey, TeamSection, Unsaved, useUnsavedWarning } from "@/components/team-settings";
import { TeamsCard } from "@/components/teams-card";
import { api, type SettingsChange } from "@/lib/api";

const field = "flex flex-col gap-1.5 text-sm font-semibold";

function useSave(done: () => void) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: SettingsChange) => api.saveSettings(c),
    meta: { inline: true },
    onSuccess: async () => (await qc.invalidateQueries(), done()),
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
  // Edits stay null until the person types, so a refetch never overwrites them.
  const [indexEdit, setIndex] = useState<string | null>(null);
  const [nameEdit, setName] = useState<string | null>(null);
  const [namesEdit, setNames] = useState<boolean | null>(null);
  const [ttlEdit, setTtl] = useState<string | null>(null);
  const saveIndex = useSave(() => setIndex(null));
  const saveYou = useSave(() => (setName(null), setNames(null)));
  const saveFresh = useSave(() => setTtl(null));
  const test = useMutation({ mutationFn: (dir: string) => api.testIndex(dir), meta: { inline: true, quiet: true } });

  const index = indexEdit ?? s?.index_dir ?? s?.index_effective ?? "";
  const name = nameEdit ?? s?.author_name ?? "";
  const names = namesEdit ?? s?.record_names ?? true;
  const ttl = ttlEdit ?? String(s?.cache_ttl_secs ?? 300);
  const dirtyIndex = !!s && indexEdit !== null && indexEdit !== (s.index_dir ?? s.index_effective);
  const dirtyYou = !!s && ((nameEdit !== null && nameEdit !== (s.author_name ?? "")) || (namesEdit !== null && namesEdit !== s.record_names));
  const dirtyFresh = !!s && ttlEdit !== null && ttlEdit !== String(s.cache_ttl_secs);
  const ttlOk = /^\d+$/.test(ttl.trim()) && Number(ttl) >= 30;
  useUnsavedWarning(dirtyIndex || dirtyYou || dirtyFresh);

  if (settings.isLoading || !s) return <Skeleton className="h-96 rounded-lg" />;
  const result = (m: typeof saveIndex) => (m.error ? <Notice ok={false} message={m.error.message} /> : null);

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <PageHeader title="Settings">
        {server ? (
          <>
            The index is <span className="font-mono text-foreground [overflow-wrap:anywhere]">{s.index_effective}</span>, set by the deployment.
          </>
        ) : (
          <>
            Saved in <span className="font-mono text-foreground [overflow-wrap:anywhere]">{s.config_path}</span>, shared with the command
            line, the MCP server and hooks.
          </>
        )}
      </PageHeader>
      <SettingsTabs />

      {!server && (
        <Section
          title="Index"
          description="Where the index lives: a folder, or a bucket location (s3://, gs:// or az://). Bucket access uses the credentials this machine or container already has."
        >
          <form
            className="contents"
            onSubmit={(e) => {
              e.preventDefault();
              if (index.trim()) saveIndex.mutate({ index_dir: index });
            }}
          >
            <label className={field}>
              Location
              <Input
                value={index}
                onChange={(e) => setIndex(e.target.value)}
                placeholder="~/comindly-index or s3://bucket/comindly"
                autoComplete="off"
                spellCheck={false}
              />
            </label>
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="outline" disabled={!index.trim()} loading={test.isPending} onClick={() => test.mutate(index)}>
                Test the location
              </Button>
              <Button type="submit" variant="brand" disabled={!index.trim()} loading={saveIndex.isPending}>
                Use this index
              </Button>
              {dirtyIndex && <Unsaved />}
            </div>
          </form>
          {test.error && <Notice ok={false} message={test.error.message} />}
          {test.data && <Notice ok message={test.data.message} />}
          {result(saveIndex)}
        </Section>
      )}

      <TeamSection />

      <TeamsCard />

      {!server && (
        <Section title="You" description="Shown on the ledger records you write and review.">
          <form
            className="contents"
            onSubmit={(e) => {
              e.preventDefault();
              saveYou.mutate({ author_name: name, record_names: names });
            }}
          >
            <label className={field}>
              Name
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={s.author_effective ?? "From git user.name"}
                autoComplete="name"
                spellCheck={false}
              />
            </label>
            <label className="flex min-h-9 items-center gap-2 text-sm">
              <input type="checkbox" checked={names} onChange={(e) => setNames(e.target.checked)} />
              Put my name on new records (off: only a pseudonymous id)
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <Button type="submit" variant="outline" loading={saveYou.isPending}>
                Save name
              </Button>
              {dirtyYou && <Unsaved />}
            </div>
          </form>
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
          <form
            className="contents"
            onSubmit={(e) => {
              e.preventDefault();
              if (ttlOk) saveFresh.mutate({ cache_ttl_secs: Number(ttl) });
            }}
          >
            <label className={field}>
              Seconds between checks
              <Input
                type="number"
                min={30}
                className="max-w-40"
                value={ttl}
                onChange={(e) => setTtl(e.target.value)}
                aria-invalid={!ttlOk}
                aria-describedby="ttl-hint"
              />
              <span id="ttl-hint" className={`text-xs font-normal ${ttlOk ? "text-muted-foreground" : "text-danger"}`}>
                {ttlOk ? "At least 30." : "Enter a whole number of at least 30."}
              </span>
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <Button type="submit" variant="outline" disabled={!ttlOk} loading={saveFresh.isPending}>
                Save interval
              </Button>
              {dirtyFresh && <Unsaved />}
            </div>
          </form>
          {result(saveFresh)}
        </Section>
      )}
    </div>
  );
}

const Claim = createContext<((placed: boolean) => void) | null>(null);

function SettingsNav() {
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const server = me.data?.mode === "server";
  const admin = me.data?.role === "admin";
  const tab = ({ isActive }: { isActive: boolean }) =>
    `flex h-8 items-center rounded-md px-3 text-xs font-semibold pointer-coarse:h-11 transition-colors duration-100 ${isActive ? "bg-card text-foreground shadow-[0_1px_2px_var(--shadow-ink)]" : "text-muted-foreground hover:text-foreground"}`;
  return (
    <nav className="flex w-fit max-w-full flex-wrap gap-0.5 rounded-lg bg-secondary p-0.5" aria-label="Settings">
      <NavLink to="/settings" end className={tab}>
        General
      </NavLink>
      <NavLink to="/settings/repositories" className={tab}>
        Repositories
      </NavLink>
      <NavLink to="/settings/prompts" className={tab}>
        Prompts
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
  );
}

/** The settings tabs, placed by a tab page right below its header. */
export function SettingsTabs() {
  const claim = useContext(Claim);
  useLayoutEffect(() => {
    claim?.(true);
    return () => claim?.(false);
  }, [claim]);
  return claim ? <SettingsNav /> : null;
}

/** Settings with its tabs; pages that don't place `SettingsTabs` get them above. */
export function SettingsShell() {
  const [placed, setPlaced] = useState(false);
  return (
    <Claim.Provider value={setPlaced}>
      <div className="flex flex-col gap-6">
        {!placed && <SettingsNav />}
        <Outlet />
      </div>
    </Claim.Provider>
  );
}
