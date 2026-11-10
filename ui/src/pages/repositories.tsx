import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Building2,
  CheckCircle2,
  Clock,
  FolderOpen,
  FolderSearch,
  GitBranch,
  Loader2,
  Play,
  Plus,
  Trash2,
  Users,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { TeamChips, TeamsField } from "@/components/team-chips";
import { RepoDot } from "@/components/symbol";
import { Link } from "react-router";
import { IndexLog } from "@/components/index-log";
import { AgentView } from "@/components/agent-view";
import { PageHeader } from "@/components/page-header";
import { TeamsCard } from "@/components/teams-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Outcome } from "@/components/notice";
import { ConfirmButton } from "@/components/confirm";
import { ago, bytes, compact, date } from "@/lib/utils";
import { useAbort } from "@/lib/abort";
import { useAccess } from "@/lib/me";
import { ApiError, api, type NeedsToken, pickFolder, type PromptTaskName, type ScanHit, type TeamSettings } from "@/lib/api";
import { useTeams } from "@/lib/teams";

const field = "flex flex-col gap-1.5 text-sm font-semibold";

/** A path input with a native folder picker in the desktop app. */
function FolderInput({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  return (
    <div className="flex gap-2">
      <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} spellCheck={false} autoComplete="off" className="font-mono text-xs" />
      {pickFolder && (
        <Button
          variant="outline"
          onClick={async () => {
            const p = await pickFolder!();
            if (p) onChange(p);
          }}
        >
          <FolderOpen /> Choose…
        </Button>
      )}
    </div>
  );
}

function Message({ error, data }: { error: Error | null; data?: { message: string } }) {
  return <Outcome error={error} message={data?.message} />;
}

function AddDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const qc = useQueryClient();
  const [path, setPath] = useState("");
  const [teams, setTeams] = useState<string[]>([]);
  const add = useMutation({
    mutationFn: () => api.changeRepos({ add: [path], teams }),
    meta: { inline: true },
    onSuccess: () => (qc.invalidateQueries({ queryKey: ["repos"] }), qc.invalidateQueries({ queryKey: ["team"] }), setPath(""), setTeams([]), onOpenChange(false)),
  });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="p-5">
        <DialogTitle className="text-[17px] font-semibold">Add a repository</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">Your local clone. comindly indexes its default branch from its own checkout.</p>
        <div className="mt-4 flex flex-col gap-3">
          <label className={field}>
            Folder
            <FolderInput value={path} onChange={setPath} placeholder="~/dev/my-service" />
          </label>
          <TeamsField value={teams} onChange={setTeams} />
          <Message error={add.error} />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="brand" disabled={!path.trim()} loading={add.isPending} onClick={() => add.mutate()}>
              Add
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ScanDialog({ open, onOpenChange, indexed }: { open: boolean; onOpenChange: (o: boolean) => void; indexed: string[] }) {
  const qc = useQueryClient();
  const [dir, setDir] = useState("");
  const [found, setFound] = useState<ScanHit[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [teams, setTeams] = useState<string[]>([]);
  const abort = useAbort();
  const scan = useMutation({
    mutationFn: () => api.scanRepos(dir, abort.next()),
    meta: { inline: true, quiet: true },
    onSuccess: (r) => (
      setFound(r.found),
      setChecked(new Set(r.found.filter((f) => f.listed || indexed.includes(f.name)).map((f) => f.path)))
    ),
  });
  const close = (o: boolean) => (o || (abort.abort(), scan.reset()), onOpenChange(o));
  const apply = useMutation({
    meta: { inline: true },
    mutationFn: () =>
      api.changeRepos({
        add: (found ?? []).filter((f) => checked.has(f.path) && !f.listed).map((f) => f.path),
        remove: (found ?? []).filter((f) => !checked.has(f.path) && f.listed).map((f) => f.path),
        teams,
      }),
    onSuccess: () => (qc.invalidateQueries({ queryKey: ["repos"] }), qc.invalidateQueries({ queryKey: ["team"] }), onOpenChange(false), setFound(null), setTeams([])),
  });
  const shown = (found ?? []).filter((f) => !filter || f.name.toLowerCase().includes(filter.toLowerCase()));
  const toggle = (p: string) =>
    setChecked((c) => {
      const n = new Set(c);
      if (n.has(p)) n.delete(p);
      else n.add(p);
      return n;
    });
  const adds = (found ?? []).filter((f) => checked.has(f.path) && !f.listed).length;
  const drops = (found ?? []).filter((f) => !checked.has(f.path) && f.listed).length;

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="top-[8%] max-h-[84dvh] overflow-y-auto p-5">
        <DialogTitle className="text-[17px] font-semibold">Scan a folder for repositories</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">Tick the ones comindly should index; untick to drop one from the list.</p>
        <div className="mt-4 flex flex-col gap-3">
          <label className={field}>
            Folder
            <FolderInput value={dir} onChange={setDir} placeholder="~/dev" />
          </label>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" disabled={!dir.trim()} loading={scan.isPending} onClick={() => scan.mutate()}>
              {!scan.isPending && <FolderSearch />} {scan.isPending ? "Scanning…" : "Scan"}
            </Button>
            {scan.isPending && (
              <Button variant="ghost" size="sm" onClick={() => (abort.abort(), scan.reset())}>
                Cancel
              </Button>
            )}
          </div>
          <Message error={scan.error} />
          {found && (
            <>
              <div className="flex items-center gap-2">
                <Input
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder={`Filter ${found.length} repositories`}
                  aria-label="Filter repositories"
                  spellCheck={false}
                  className="h-8 text-xs"
                />
                <Button variant="ghost" size="sm" onClick={() => setChecked(new Set([...checked, ...shown.map((f) => f.path)]))}>
                  Select all
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setChecked(new Set([...checked].filter((p) => !shown.some((f) => f.path === p))))}
                >
                  Select none
                </Button>
              </div>
              <div className="max-h-72 overflow-y-auto overscroll-contain rounded-md bg-muted/50">
                {shown.length === 0 && <p className="p-3 text-xs text-muted-foreground">No git repositories found.</p>}
                {shown.map((f) => (
                  <label
                    key={f.path}
                    className="flex cursor-pointer items-center gap-2.5 border-b px-3 py-2 text-sm last:border-0 hover:bg-muted/50"
                  >
                    <input type="checkbox" checked={checked.has(f.path)} onChange={() => toggle(f.path)} />
                    <span className="min-w-0 truncate font-medium">{f.name}</span>
                    {f.listed && <Badge variant="outline">Added</Badge>}
                    <span className="ml-auto min-w-0 truncate font-mono text-xs text-muted-foreground max-sm:hidden">{f.remote ?? f.path}</span>
                  </label>
                ))}
              </div>
              {adds > 0 && <TeamsField value={teams} onChange={setTeams} />}
              <Message error={apply.error} />
              <div className="flex items-center justify-end gap-2">
                <span className="mr-auto text-xs text-muted-foreground">
                  {adds} to add, {drops} to remove
                </span>
                <Button variant="ghost" onClick={() => close(false)}>
                  Cancel
                </Button>
                <Button variant="brand" disabled={adds + drops === 0} loading={apply.isPending} onClick={() => apply.mutate()}>
                  Apply
                </Button>
              </div>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** The LLM steps an index can run, with what each sends to the LLM. */
const STEPS: { task: PromptTaskName; label: string; setting: "enrich" | "flows" | "guide" }[] = [
  { task: "summary", label: "Summaries (sends each symbol's code)", setting: "enrich" },
  { task: "flow", label: "Walkthroughs (sends call traces and code)", setting: "flows" },
  { task: "guide", label: "Style guides (sends code samples and config)", setting: "guide" },
];

function IndexPanel({ unlisted }: { unlisted: string[] }) {
  const qc = useQueryClient();
  // Each step starts as the team settings have it, once they are known; null until changed here.
  const [picked, setPicked] = useState<PromptTaskName[] | null>(null);
  const [drop, setDrop] = useState(false);
  const job = useQuery({
    queryKey: ["index-job"],
    queryFn: api.indexJob,
    refetchInterval: (q) => (q.state.data?.running || q.state.data?.queued?.length ? 1000 : false),
  });
  const start = useMutation({
    mutationFn: () => api.startIndex(steps.length > 0, drop, [], steps),
    meta: { inline: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["index-job"] }),
  });
  const stop = useMutation({
    mutationFn: api.cancelIndex,
    meta: { quiet: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["index-job"] }),
  });
  const dequeue = useMutation({
    mutationFn: api.cancelJob,
    meta: { quiet: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["index-job"] }),
  });
  const j = job.data;
  const queued = j?.queued ?? [];
  const [seen, setSeen] = useState<string | null>(null);
  useEffect(() => {
    if (j && !j.running && j.ok && j.finished && j.finished !== seen) {
      setSeen(j.finished);
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "index-job" });
    }
  }, [j, seen, qc]);
  const watch = useQuery({ queryKey: ["index-watch"], queryFn: api.indexWatch, refetchInterval: 30_000 });
  const team = useQuery({ queryKey: ["team"], queryFn: api.team });
  const steps = picked ?? (team.data ? STEPS.filter((s) => team.data.settings[s.setting] ?? true).map((s) => s.task) : []);
  const llm = steps.length > 0;
  const [minutes, setMinutes] = useState<string | null>(null);
  const auto = useMutation({
    mutationFn: (s: Pick<TeamSettings, "auto_index" | "auto_index_llm" | "auto_index_minutes">) =>
      api.saveTeam({ ...team.data!.settings, ...s }, team.data!.version),
    onSuccess: () => (setMinutes(null), qc.invalidateQueries({ queryKey: ["team"] }), qc.invalidateQueries({ queryKey: ["index-watch"] })),
  });
  const every = minutes ?? String(team.data?.settings.auto_index_minutes ?? watch.data?.minutes ?? 5);
  const everyValid = /^\d+$/.test(every) && Number(every) >= 1 && Number(every) <= 1440;

  return (
    <Card>
      <CardHeader className="flex-row flex-wrap items-start justify-between gap-4 pb-3">
        <div className="max-w-[68ch]">
          <h2 className="text-sm font-semibold">Index</h2>
          <CardDescription className="mt-1 text-sm">
            Fetches each repository's default branch into comindly's own checkout, then rebuilds the index. Unchanged code reuses its
            embeddings{llm ? " and what the LLM wrote" : ""}.
          </CardDescription>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-2">
          <div className="flex gap-2">
            {j?.running && (
              <Button variant="outline" loading={stop.isPending} onClick={() => stop.mutate()}>
                Cancel
              </Button>
            )}
            <Button
              variant="brand"
              disabled={(unlisted.length > 0 && !drop) || queued.length > 0}
              loading={start.isPending}
              onClick={() => start.mutate()}
              title={j?.running ? "Index again once the running job ends" : undefined}
            >
              {!start.isPending && <Play />} {j?.running ? "Queue next" : "Index now"}
            </Button>
          </div>
          <fieldset className="flex flex-col items-start gap-0.5">
            <legend className="sr-only">LLM steps for this index</legend>
            {STEPS.map((s) => (
              <label key={s.task} className="flex min-h-8 items-center gap-1.5 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={steps.includes(s.task)}
                  onChange={(e) => setPicked(e.target.checked ? [...steps, s.task] : steps.filter((t) => t !== s.task))}
                />
                {s.label}
              </label>
            ))}
          </fieldset>
          {unlisted.length > 0 && (
            <label className="flex items-center gap-1.5 text-xs text-warning">
              <input type="checkbox" checked={drop} onChange={(e) => setDrop(e.target.checked)} />
              Drop {unlisted.length === 1 ? unlisted[0] : `${unlisted.length} repositories`} with no folder here
            </label>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 border-t pt-4">
        <label className="flex min-h-8 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={team.data?.settings.auto_index ?? false}
            disabled={!team.data || auto.isPending}
            onChange={(e) => auto.mutate({ auto_index: e.target.checked })}
          />
          Re-index automatically when a default branch moves
        </label>
        <div className="ml-6 flex flex-wrap items-center gap-2 text-sm">
          Check every
          <Input
            type="number"
            min={1}
            max={1440}
            className="h-8 w-20"
            value={every}
            disabled={!team.data || auto.isPending || !team.data.settings.auto_index}
            onChange={(e) => setMinutes(e.target.value)}
            aria-label="Minutes between checks"
          />
          minutes
          {minutes !== null && (
            <Button size="sm" variant="outline" disabled={!everyValid} loading={auto.isPending} onClick={() => auto.mutate({ auto_index_minutes: Number(every) })}>
              Save
            </Button>
          )}
          {minutes !== null && !everyValid && <span className="text-xs text-warning">Between 1 and 1440 minutes</span>}
        </div>
        <label className="ml-6 flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={team.data?.settings.auto_index_llm ?? false}
            disabled={!team.data || auto.isPending || !team.data.settings.auto_index}
            onChange={(e) => auto.mutate({ auto_index_llm: e.target.checked })}
          />
          With LLM steps: summaries, style guides and walkthroughs for new or changed code only
        </label>
        <Message error={auto.error} />
        {watch.data?.enabled && watch.data.last.checked && (
          <p className="text-xs text-muted-foreground">
            Last check {ago(watch.data.last.checked)}: {watch.data.last.result}
          </p>
        )}
        <Message error={start.error} />
        {j && !j.running && j.ok !== null && (
          <p className={`flex items-center gap-1.5 text-sm ${j.ok ? "" : "text-warning"}`}>
            {j.ok ? <CheckCircle2 className="size-4 text-brand" /> : <AlertTriangle className="size-4" />}
            {j.ok ? "Indexed; every page shows the new index." : j.cancelled ? "Indexing cancelled." : "Indexing failed; see the log."}
            {j.finished && (
              <span className="text-xs text-muted-foreground" title={new Date(j.finished).toLocaleString()}>
                {ago(j.finished)}
              </span>
            )}
          </p>
        )}
        {queued.map((q) => (
          <p key={q.id} className="flex flex-wrap items-center gap-1.5 text-sm">
            <Clock className="size-4 text-muted-foreground" />
            Queued{queued.length > 1 ? ` (position ${q.position})` : ""}: indexing starts when the running job ends.
            <Button variant="outline" size="sm" loading={dequeue.isPending} onClick={() => dequeue.mutate(q.id)}>
              Cancel
            </Button>
          </p>
        ))}
        {!!j?.events.length && (
          <div className="mt-2 flex flex-col gap-2">
            <IndexLog
              events={j.events}
              running={j.running}
              status={j.running ? "running" : j.cancelled ? "cancelled" : j.ok === false ? "failed" : "succeeded"}
              started={j.started}
              finished={j.finished}
              summary={false}
            />
            <Link to={j.id ? `/activity/${j.id}` : "/activity"} className="self-start text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground">
              All runs under Activity
            </Link>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/** Pick repositories from a GitHub organisation or GitLab group; comindly clones them itself. */
function OrgDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const qc = useQueryClient();
  const { server } = useAccess();
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const hasTeam = team.data?.version != null;
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [token, setToken] = useState("");
  const [asked, setAsked] = useState<NeedsToken | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [share, setShare] = useState(true);
  const [teams, setTeams] = useState<string[]>([]);
  const abort = useAbort();
  const list = useMutation({
    mutationFn: () => api.remoteRepos(url, asked && token.trim() ? { username, token } : undefined, abort.next()),
    meta: { inline: true, quiet: true },
    onSuccess: (r) => (setAsked(null), setChecked(new Set(r.repos.filter((x) => !x.archived && !x.listed).map((x) => x.clone_url)))),
    onError: (e) => {
      const need = e instanceof ApiError && e.body.code === "needs_token" ? (e.body as unknown as NeedsToken) : undefined;
      if (need) (setAsked(need), setUsername((u) => u || need.username));
    },
  });
  const close = (o: boolean) => (o || (abort.abort(), list.reset()), onOpenChange(o));
  const add = useMutation({
    mutationFn: () => api.addRemoteRepos([...checked], hasTeam && share, teams),
    meta: { inline: true },
    onSuccess: () => (qc.invalidateQueries({ queryKey: ["repos"] }), qc.invalidateQueries({ queryKey: ["team"] }), close(false)),
  });
  const found = list.data?.repos ?? [];
  const shown = found.filter((r) => !filter || `${r.full_path} ${r.description ?? ""}`.toLowerCase().includes(filter.toLowerCase()));
  const toggle = (u: string) =>
    setChecked((c) => {
      const n = new Set(c);
      if (n.has(u)) n.delete(u);
      else n.add(u);
      return n;
    });
  const picked = found.filter((r) => checked.has(r.clone_url) && !r.listed).length;

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="max-w-2xl p-5">
        <DialogTitle className="text-[17px] font-semibold">From GitHub or GitLab</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">
          An organisation or group (github.com/acme, gitlab.com/acme/platform), or one repository (github.com/acme/orders, or a clone URL).
          comindly keeps its own copies and always indexes each repository's default branch.
        </p>
        <label htmlFor="org-url" className={`${field} mt-4`}>
          Organisation, group or repository
        </label>
        <div className="mt-1.5 flex gap-2">
          <Input
            id="org-url"
            value={url}
            onChange={(e) => (setUrl(e.target.value), setAsked(null))}
            onKeyDown={(e) => e.key === "Enter" && url.trim() && list.mutate()}
            placeholder="github.com/acme or github.com/acme/orders"
            spellCheck={false}
            autoComplete="off"
            className="font-mono text-xs"
          />
          {list.isPending ? (
            <Button variant="outline" onClick={() => (abort.abort(), list.reset())}>
              <Loader2 className="animate-spin" /> Cancel
            </Button>
          ) : (
            <Button variant="outline" disabled={!url.trim() || (!!asked && !token.trim())} onClick={() => list.mutate()}>
              List
            </Button>
          )}
        </div>
        {asked && (
          <div className="mt-3 flex flex-col gap-2 rounded-md bg-muted p-3">
            <p className="text-xs text-muted-foreground">
              {asked.host} needs an access token that can read these repositories.{" "}
              {server
                ? "comindly keeps it encrypted on this server (Settings → Integrations)."
                : "Your git credential helper keeps it afterwards."}
            </p>
            <div className="grid gap-2 sm:grid-cols-[10rem_1fr]">
              <label className="flex flex-col gap-1 text-xs font-semibold">
                Username
                <Input value={username} onChange={(e) => setUsername(e.target.value)} spellCheck={false} autoComplete="username" className="text-xs" />
              </label>
              <label className="flex flex-col gap-1 text-xs font-semibold">
                Access token
                <Input type="password" value={token} onChange={(e) => setToken(e.target.value)} spellCheck={false} autoComplete="off" autoFocus />
              </label>
            </div>
          </div>
        )}
        {!asked && (
          <div className="mt-2">
            <Message error={list.error} />
          </div>
        )}
        {list.data?.warning && <p className="mt-2 text-xs text-warning">{list.data.warning}</p>}
        {found.length > 0 && (
          <div className="mt-4 flex flex-col gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <Input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder={`Filter ${found.length} repositories`}
                aria-label="Filter repositories"
                spellCheck={false}
                className="h-8 text-xs sm:max-w-xs"
              />
              <Button variant="ghost" size="sm" onClick={() => setChecked(new Set(shown.filter((r) => !r.listed).map((r) => r.clone_url)))}>
                Select all
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setChecked(new Set())}>
                Select none
              </Button>
            </div>
            <div className="max-h-80 overflow-y-auto overscroll-contain rounded-md bg-muted/50">
              {shown.map((r) => (
                <label
                  key={r.clone_url}
                  className={`flex cursor-pointer items-start gap-3 border-b px-3 py-2 last:border-0 ${r.listed ? "opacity-60" : ""}`}
                >
                  <input
                    type="checkbox"
                    className="mt-1"
                    disabled={r.listed}
                    checked={r.listed || checked.has(r.clone_url)}
                    onChange={() => toggle(r.clone_url)}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="text-sm font-medium">{r.name}</span>
                      <span className="truncate font-mono text-xs text-muted-foreground">{r.full_path}</span>
                      {r.listed && <Badge variant="outline">Added</Badge>}
                      {r.archived && <Badge variant="outline">Archived</Badge>}
                      {r.fork && <Badge variant="outline">Fork</Badge>}
                    </div>
                    {r.description && <div className="truncate text-xs text-muted-foreground">{r.description}</div>}
                  </div>
                  <div className="shrink-0 text-right text-xs text-muted-foreground">
                    {r.last_activity && <div>{date(r.last_activity)}</div>}
                    {r.size_kib != null && <div>{bytes(r.size_kib)}</div>}
                  </div>
                </label>
              ))}
            </div>
            {picked > 0 && <TeamsField value={teams} onChange={setTeams} />}
            <div className="mt-1 flex flex-wrap items-center justify-between gap-3">
              {hasTeam ? (
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={share} onChange={(e) => setShare(e.target.checked)} />
                  Share with the org
                </label>
              ) : (
                <span />
              )}
              <Button variant="brand" disabled={picked === 0} loading={add.isPending} onClick={() => add.mutate()}>
                Add {picked} {picked === 1 ? "repository" : "repositories"}
              </Button>
            </div>
            <Message error={add.error} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function Symbols({ n }: { n: number }) {
  return (
    <span className="nums" title={`${n.toLocaleString()} symbols`}>
      {compact(n)} symbols
    </span>
  );
}

/** One repository: name and status, then what is indexed, then where it lives; actions at the end. */
function RepoRow({
  name,
  status,
  meta,
  where,
  actions,
  teams,
}: {
  name: string;
  status: ReactNode;
  meta: ReactNode;
  where?: string | null;
  actions?: ReactNode;
  teams?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start gap-x-3 gap-y-2 border-b px-5 py-3 last:border-0 max-sm:px-4">
      <div className="min-w-0 flex-1 basis-48">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <span className="flex min-w-0 items-center gap-2">
            <RepoDot repo={name} />
            <span className="truncate font-semibold">{name}</span>
          </span>
          {status}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">{meta}</div>
        {where && (
          <div className="mt-0.5 truncate font-mono text-xs text-muted-foreground" title={where}>
            {where}
          </div>
        )}
        {teams}
      </div>
      {actions && <div className="ml-auto flex flex-wrap justify-end gap-1.5">{actions}</div>}
    </div>
  );
}

export function RepositoriesPage() {
  const qc = useQueryClient();
  const { server, admin } = useAccess();
  const repos = useQuery({ queryKey: ["repos"], queryFn: api.repos });
  const teams = useTeams();
  const shown = (repos.data?.repos ?? []).filter((r) => teams.inView(r.name));
  const hidden = (repos.data?.repos.length ?? 0) - shown.length;
  const [dialog, setDialog] = useState<"add" | "scan" | "org" | null>(null);
  const teamMissing = repos.data?.team_missing ?? [];
  const addTeam = useMutation({
    mutationFn: () => api.addRemoteRepos(teamMissing, false),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
  const remove = useMutation({
    mutationFn: (path: string) => api.changeRepos({ remove: [path] }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
  const addOne = useMutation({
    mutationFn: (url: string) => api.addRemoteRepos([url], false),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
  const addAll = useMutation({
    mutationFn: (urls: string[]) => api.addRemoteRepos(urls, false),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
  const remote = useMemo(() => repos.data?.root?.includes("://"), [repos.data]);
  const unlisted = repos.data?.unlisted ?? [];
  const close = (o: boolean) => !o && setDialog(null);

  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Repositories" actions={<AgentView asks={[{ label: "Repositories", tool: "repos", args: { team: teams.selected.join(",") || undefined } }]} />}>
        What comindly indexes. Each repository is indexed at its default branch from comindly's own checkout
        {server ? "." : ", so your folders and branches are never touched."}
      </PageHeader>
      <div className="flex flex-wrap items-center justify-end gap-3">
        {admin && (
          <div className="grid w-full grid-cols-2 gap-2 sm:flex sm:w-auto sm:flex-wrap">
            {!server && (
              <Button variant="outline" onClick={() => setDialog("add")}>
                <Plus /> Add a folder…
              </Button>
            )}
            <Button variant="brand" className="col-span-2 max-sm:order-first" onClick={() => setDialog("org")}>
              <Building2 /> Add from GitHub or GitLab…
            </Button>
            {!server && (
              <Button variant="outline" onClick={() => setDialog("scan")}>
                <FolderSearch /> Scan a folder…
              </Button>
            )}
          </div>
        )}
      </div>

      {admin && teamMissing.length > 0 && (
        <div className="flex flex-wrap items-center gap-3 rounded-md bg-accent px-4 py-3 text-sm">
          <Users className="size-4 shrink-0 text-brand" />
          <span className="min-w-0 flex-1">
            The org has {teamMissing.length === 1 ? "a repository" : `${teamMissing.length} repositories`} you don't have here yet.
          </span>
          <Button variant="outline" size="sm" loading={addTeam.isPending} onClick={() => addTeam.mutate()}>
            Add {teamMissing.length === 1 ? "it" : "them"}
          </Button>
        </div>
      )}

      {unlisted.length > 0 ? (
        <div className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-2 gap-y-3 rounded-md bg-warning/10 px-4 py-3 text-sm md:grid-cols-[auto_minmax(0,1fr)_auto]">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warning" />
          <span className="min-w-0 [overflow-wrap:anywhere]">
            {unlisted.length === 1 ? "One repository" : `${unlisted.length} repositories`} in this index{" "}
            {unlisted.length === 1 ? "has" : "have"} no folder {server ? "on this server" : "on this machine"} yet.{" "}
            {server ? "Add one and comindly clones it." : "Locate your copy, or add it and comindly clones it."} All repositories are
            indexed together, so their links stay complete.
          </span>
          {admin && unlisted.filter((r) => r.remote).length > 1 && (
            <Button
              variant="outline"
              size="sm"
              className="col-start-2 justify-self-start md:col-start-auto"
              loading={addAll.isPending}
              onClick={() => addAll.mutate(unlisted.flatMap((r) => (r.remote ? [r.remote] : [])))}
            >
              <Plus /> Add all {unlisted.filter((r) => r.remote).length}
            </Button>
          )}
        </div>
      ) : (
        remote &&
        !server && (
          <p className="text-muted-foreground">
            Shared in a bucket: indexing from here publishes a new version for everyone using it.
          </p>
        )
      )}

      <Card className="overflow-hidden">
        {repos.isLoading && <Skeleton className="h-32" />}
        {repos.data && repos.data.repos.length === 0 && unlisted.length === 0 && (
          <p className="px-5 py-8 text-muted-foreground">
            {!admin
              ? "No repositories yet. An admin adds them."
              : server
                ? "No repositories yet. Add them from GitHub or GitLab."
                : "No repositories yet. Add a folder, scan one, or add from GitHub or GitLab."}
          </p>
        )}
        {hidden > 0 && (
          <p className="flex flex-wrap items-center gap-2 border-b px-5 py-2.5 text-xs text-muted-foreground max-sm:px-4">
            {hidden === 1 ? "One repository is" : `${hidden} repositories are`} outside the teams in view.
            <button className="underline underline-offset-2 hover:text-foreground" onClick={() => teams.select([])}>
              Show everyone's
            </button>
          </p>
        )}
        {shown.map((r) => (
          <RepoRow
            key={r.path}
            name={r.name}
            teams={teams.setUp || admin ? <TeamChips repo={r.name} admin={admin} /> : undefined}
            status={
              <>
                {r.managed && !r.cloned && !r.indexed_commit && <Badge variant="outline">Cloned when indexing</Badge>}
                {!r.managed && !r.exists && <Badge variant="danger">Folder missing</Badge>}
                {r.symbols == null && <Badge variant="outline">Not indexed yet</Badge>}
              </>
            }
            meta={
              <>
                {r.symbols != null && <Symbols n={r.symbols} />}
                <span className="flex items-center gap-1" title="What comindly indexes">
                  <GitBranch className="size-3.5 shrink-0" />
                  {r.branch ? <span className="font-mono text-foreground">origin/{r.branch}</span> : "Default branch"}
                  {r.indexed_commit && <span className="font-mono text-foreground">{r.indexed_commit.slice(0, 7)}</span>}
                </span>
                {!r.managed && (
                  <span title="Your own checkout, left as it is">
                    Yours: <span className="font-mono">{r.your_branch ?? "detached"}</span>
                  </span>
                )}
              </>
            }
            where={r.managed ? r.remote : r.path}
            actions={
              <>
                <AgentView icon asks={[{ label: "Conventions", tool: "guide", args: { focus: r.name } }]} />
                {admin && (
                  <ConfirmButton
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    aria-label={`Remove ${r.name}`}
                    title="Remove from the list (your folder stays)"
                    question={`Remove ${r.name}?`}
                    confirmLabel="Remove"
                    loading={remove.isPending && remove.variables === r.path}
                    onConfirm={() => remove.mutate(r.path)}
                  >
                    {!(remove.isPending && remove.variables === r.path) && <Trash2 />}
                  </ConfirmButton>
                )}
              </>
            }
          />
        ))}
        {unlisted.map((r) => (
          <RepoRow
            key={r.name}
            name={r.name}
            teams={teams.setUp || admin ? <TeamChips repo={r.name} admin={admin} /> : undefined}
            status={
              (repos.data?.repos.length ?? 0) > 0 && <span className="text-xs whitespace-nowrap text-muted-foreground">{server ? "Not on this server" : "Not on this machine"}</span>
            }
            meta={
              <>
                {r.symbols != null && <Symbols n={r.symbols} />}
                {r.commit && (
                  <span>
                    Indexed at <span className="font-mono text-foreground">{r.commit.slice(0, 7)}</span>
                  </span>
                )}
              </>
            }
            where={r.remote ?? "remote unknown (indexed before comindly recorded remotes)"}
            actions={
              admin && (
                <>
                  {!server && (
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-8"
                      aria-label={`Locate ${r.name}`}
                      title="Locate your copy of it on this machine"
                      onClick={() => setDialog("add")}
                    >
                      <FolderOpen />
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    aria-label={`Add ${r.name}`}
                    disabled={!r.remote}
                    title={r.remote ? "Add it: comindly clones it into its own folder when indexing" : "The index has no clone URL for it"}
                    loading={addOne.isPending && addOne.variables === r.remote}
                    onClick={() => r.remote && addOne.mutate(r.remote)}
                  >
                    {!(addOne.isPending && addOne.variables === r.remote) && <Plus />}
                  </Button>
                </>
              )
            }
          />
        ))}
      </Card>

      {admin && <IndexPanel unlisted={unlisted.map((r) => r.name)} />}
      <TeamsCard />

      <AddDialog open={dialog === "add"} onOpenChange={close} />
      <ScanDialog open={dialog === "scan"} onOpenChange={close} indexed={unlisted.map((r) => r.name)} />
      <OrgDialog open={dialog === "org"} onOpenChange={close} />
    </div>
  );
}
