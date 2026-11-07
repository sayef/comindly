import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Outcome } from "@/components/notice";
import { useAccess } from "@/lib/me";
import { api, type KeySource, type TeamSettings } from "@/lib/api";

const field = "flex flex-col gap-1.5 text-sm font-semibold";

/** Asks the browser to warn before leaving the page while `dirty`. */
export function useUnsavedWarning(dirty: boolean) {
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);
}

export const Unsaved = () => <span className="text-xs text-muted-foreground">Unsaved changes</span>;

const sources: Record<KeySource, string> = {
  team: "From your credentials file, for this org",
  default: "From your credentials file",
  environment: "From OPENAI_API_KEY in the environment",
  server: "Stored on this server (Settings → Integrations)",
  none: "Not set",
};

/** Your own LLM API key, kept in your credentials file on this machine and never in the index. */
export function ApiKey() {
  const qc = useQueryClient();
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings });
  const source = team.data?.key_source ?? settings.data?.llm_key ?? "none";
  const name = team.data?.settings.name;
  const [key, setKey] = useState("");
  const save = useMutation({
    mutationFn: () => api.saveKey(key),
    meta: { inline: true },
    onSuccess: () => (setKey(""), qc.invalidateQueries({ queryKey: ["team"] }), qc.invalidateQueries({ queryKey: ["settings"] })),
  });
  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm font-semibold">
        <span className="flex items-center gap-2 whitespace-nowrap">
          <KeyRound className="size-4" /> Your API key
        </span>
        <Badge variant={source === "none" ? "outline" : "brand"}>{sources[source]}</Badge>
      </p>
      <p className="text-xs text-muted-foreground">
        Saved in <code className="font-mono">~/.config/comindly/credentials</code>{name ? ` under ${name}` : ""}, readable only by you; otherwise <code className="font-mono">OPENAI_API_KEY</code> is used.
      </p>
      <form
        className="flex flex-wrap gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (key.trim()) save.mutate();
        }}
      >
        <Input
          type="password"
          className="min-w-0 flex-1 basis-48 sm:max-w-sm"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="Paste your API key…"
          aria-label="API key"
          autoComplete="off"
          spellCheck={false}
        />
        <Button type="submit" variant="outline" disabled={!key.trim()} loading={save.isPending}>
          Save key
        </Button>
      </form>
      <Outcome error={save.error} message={save.data?.message} />
    </div>
  );
}

const same = (a: TeamSettings, b: TeamSettings) => {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)] as (keyof TeamSettings)[]);
  return [...keys].every((k) => (a[k] ?? "") === (b[k] ?? ""));
};

/** The index's shared settings: `org` holds its name and how agents get text, `models` the LLM and embedding setup. */
export function TeamSection({ part }: { part: "org" | "models" }) {
  const qc = useQueryClient();
  const { server, admin } = useAccess();
  const team = useQuery({ queryKey: ["team"], queryFn: api.team });
  const t = team.data;
  const d = t?.defaults;
  // Null until the person edits, so a refetch never overwrites unsaved edits.
  const [edits, setEdits] = useState<TeamSettings | null>(null);
  const form = edits ?? t?.settings ?? {};
  const dirty = !!t && edits !== null && !same(edits, t.settings);
  useUnsavedWarning(dirty);
  const save = useMutation({
    mutationFn: () => api.saveTeam(form, t?.version ?? null),
    meta: { inline: true },
    onSuccess: async () => {
      await Promise.all([qc.invalidateQueries({ queryKey: ["team"] }), qc.invalidateQueries({ queryKey: ["settings"] })]);
      setEdits(null);
    },
  });
  const set = (k: keyof TeamSettings, v: string | number | boolean | undefined) => setEdits((f) => ({ ...(f ?? t?.settings), [k]: v }));
  const num = (k: "max_enrich" | "max_flows" | "llm_concurrency", v: string) =>
    set(k, v.trim() === "" ? undefined : Math.max(1, Number(v)));
  const price = (k: "llm_price_in" | "llm_price_out", v: string) => set(k, v.trim() === "" ? undefined : Math.max(0, Number(v)));
  const steps: [keyof TeamSettings, string][] = [
    ["embed", "Search embeddings"],
    ["enrich", "Summaries"],
    ["flows", "Flow walkthroughs"],
    ["guide", "Style guide"],
  ];
  const taskModels: [keyof TeamSettings, string][] = [
    ["llm_model_summary", "Summaries"],
    ["llm_model_guide", "Style guides"],
    ["llm_model_flow", "Walkthroughs"],
  ];
  const text = (k: keyof TeamSettings, label: string, placeholder?: string, type = "text") => (
    <label className={field}>
      {label}
      <Input type={type} value={(form[k] as string | undefined) ?? ""} onChange={(e) => set(k, e.target.value)} placeholder={placeholder} autoComplete="off" spellCheck={false} />
    </label>
  );

  if (!t) return null;
  const who = server ? " Only admins change them." : " Changing them needs write access to the index.";
  return (
    <Card>
      <CardHeader>
        <CardTitle>{part === "org" ? "Org" : "Models"}</CardTitle>
        <CardDescription>
          {part === "org"
            ? `Stored with the index and used by everyone on it.${who}`
            : `The LLM and embedding model everyone on this index uses${server ? ", with the key from Integrations" : "; each person brings their own API key"}. Empty fields use comindly's default, shown as the placeholder.${who}`}
          {t.changed_by && (
            <span>
              {" "}
              Last changed by {t.changed_by}
              {t.changed_at ? `, ${new Date(t.changed_at).toLocaleString()}` : ""}.
            </span>
          )}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (admin) save.mutate();
          }}
        >
          <fieldset disabled={!admin} className="contents">
            {part === "org" ? (
              <div className="grid gap-3 sm:grid-cols-2">
                <label className={field}>
                  Org name
                  <Input value={form.name ?? ""} onChange={(e) => set("name", e.target.value)} placeholder="e.g. Acme" autoComplete="off" />
                </label>
                <label className={field}>
                  Agents get text as
                  <select
                    className="h-9 rounded-md border border-input bg-card px-2 text-sm"
                    value={form.format ?? "md"}
                    onChange={(e) => set("format", e.target.value === "md" ? undefined : e.target.value)}
                  >
                    <option value="md">Markdown (reads best)</option>
                    <option value="json">JSON</option>
                  </select>
                  <span className="text-xs font-normal text-muted-foreground">Every answer also carries the same data as structured JSON.</span>
                </label>
              </div>
            ) : (
              <>
                <div className="grid gap-3 sm:grid-cols-2">
                  {text("llm_base_url", "LLM endpoint", d?.llm_base_url, "url")}
                  {text("llm_model", "LLM model", d?.llm_model)}
                </div>
                <div className="flex flex-col gap-2">
                  <span className="text-sm font-semibold">A model per task</span>
                  <span className="text-xs text-muted-foreground">
                    For example a cheaper one for the many summaries and a stronger one for style guides. Empty uses the LLM model.
                  </span>
                  <div className="grid gap-3 sm:grid-cols-3">
                    {taskModels.map(([k, label]) => (
                      <div key={k}>{text(k, label, form.llm_model || d?.llm_model)}</div>
                    ))}
                  </div>
                </div>
                <div className="grid gap-3 sm:grid-cols-2">{text("embed_model", "Embedding model", d?.embed_model)}</div>
                <div className="flex flex-wrap gap-x-5 gap-y-1 text-sm">
                  {steps.map(([k, label]) => (
                    <label key={k} className="flex min-h-9 items-center gap-2">
                      <input type="checkbox" checked={form[k] !== false} onChange={(e) => set(k, e.target.checked)} />
                      {label} when indexing
                    </label>
                  ))}
                </div>
                <div className="grid gap-3 sm:grid-cols-2">
                  <label className={field}>
                    Most symbols to summarise
                    <Input type="number" min={1} value={form.max_enrich ?? ""} onChange={(e) => num("max_enrich", e.target.value)} placeholder="no limit" />
                    <span className="text-xs font-normal text-muted-foreground">The most-used first. Each summary is one LLM request and sends that code.</span>
                  </label>
                  <label className={field}>
                    Most flows to walk through
                    <Input type="number" min={1} value={form.max_flows ?? ""} onChange={(e) => num("max_flows", e.target.value)} placeholder="no limit" />
                    <span className="text-xs font-normal text-muted-foreground">Those reaching furthest first. Every flow is still traced without the LLM.</span>
                  </label>
                  <label className={field}>
                    LLM requests at once
                    <Input
                      type="number"
                      min={1}
                      max={64}
                      value={form.llm_concurrency ?? ""}
                      onChange={(e) => num("llm_concurrency", e.target.value)}
                      placeholder={String(d?.llm_concurrency ?? 8)}
                    />
                    <span className="text-xs font-normal text-muted-foreground">Lower it if the provider rate-limits; higher indexes faster.</span>
                  </label>
                  <span />
                  <label className={field}>
                    Price per million input tokens (USD)
                    <Input type="number" min={0} step="0.01" value={form.llm_price_in ?? ""} onChange={(e) => price("llm_price_in", e.target.value)} placeholder="not set" />
                    <span className="text-xs font-normal text-muted-foreground">From your provider's price list; the indexing log then shows what it costs.</span>
                  </label>
                  <label className={field}>
                    Price per million output tokens (USD)
                    <Input type="number" min={0} step="0.01" value={form.llm_price_out ?? ""} onChange={(e) => price("llm_price_out", e.target.value)} placeholder="not set" />
                    <span className="text-xs font-normal text-muted-foreground">Without prices, the log shows tokens only.</span>
                  </label>
                </div>
              </>
            )}
            {admin && (
              <div className="flex flex-wrap items-center gap-2">
                <Button type="submit" variant="outline" loading={save.isPending}>
                  Save for the org
                </Button>
                {dirty && <Unsaved />}
              </div>
            )}
            <Outcome error={save.error} message={save.data?.message} />
          </fieldset>
        </form>
        {part === "models" && !server && <ApiKey />}
      </CardContent>
    </Card>
  );
}
