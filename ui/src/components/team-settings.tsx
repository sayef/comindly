import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Outcome } from "@/components/notice";
import { useAccess } from "@/lib/me";
import { api, type KeySource, type TeamSettings } from "@/lib/api";

const field = "flex flex-col gap-1.5 text-sm font-semibold";

const sources: Record<KeySource, string> = {
  team: "From your credentials file, for this team",
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
      <p className="flex items-center gap-2 text-sm font-semibold">
        <KeyRound className="size-4" /> Your API key
        <Badge variant={source === "none" ? "outline" : "brand"}>{sources[source]}</Badge>
      </p>
      <p className="text-xs text-muted-foreground">
        Saved in ~/.config/comindly/credentials{name ? ` under ${name}` : ""}, readable only by you; otherwise OPENAI_API_KEY is used.
      </p>
      <div className="flex flex-wrap gap-2">
        <Input type="password" className="max-w-sm" value={key} onChange={(e) => setKey(e.target.value)} placeholder="Paste your API key" />
        <Button variant="outline" size="sm" disabled={!key.trim()} loading={save.isPending} onClick={() => save.mutate()}>
          Save key
        </Button>
      </div>
      <Outcome error={save.error} message={save.data?.message} />
    </div>
  );
}

export function TeamSection() {
  const qc = useQueryClient();
  const { server, admin } = useAccess();
  const team = useQuery({ queryKey: ["team"], queryFn: api.team });
  const t = team.data;
  const d = t?.defaults;
  const [form, setForm] = useState<TeamSettings>({});
  useEffect(() => {
    if (t) setForm(t.settings);
  }, [t]);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["team"] });
    qc.invalidateQueries({ queryKey: ["settings"] });
  };
  const save = useMutation({ mutationFn: () => api.saveTeam(form, t?.version ?? null), meta: { inline: true }, onSuccess: refresh });
  const set = (k: keyof TeamSettings, v: string | number | boolean | undefined) => setForm((f) => ({ ...f, [k]: v }));
  const num = (k: "max_enrich" | "max_flows" | "llm_concurrency", v: string) =>
    set(k, v.trim() === "" ? undefined : Math.max(1, Number(v)));
  const price = (k: "llm_price_in" | "llm_price_out", v: string) => set(k, v.trim() === "" ? undefined : Math.max(0, Number(v)));
  const steps: [keyof TeamSettings, string][] = [
    ["embed", "Search embeddings"],
    ["enrich", "Summaries"],
    ["flows", "Flow walkthroughs"],
    ["guide", "Style guide"],
  ];

  if (!t) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Users className="size-4" /> Team
        </CardTitle>
        <CardDescription>
          {server
            ? "Stored with the index and used by everyone on it: the same LLM and embedding model for all, with the key from Integrations. Empty fields use comindly's default, shown as the placeholder. Only admins change them."
            : "Stored with the index and used by everyone on it: the same LLM and embedding model for all; each person brings their own API key. Empty fields use comindly's default, shown as the placeholder. Changing them needs write access to the index."}
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
        <fieldset disabled={!admin} className="contents">
          <div className="grid gap-3 sm:grid-cols-2">
            <label className={field}>
              Team name
              <Input value={form.name ?? ""} onChange={(e) => set("name", e.target.value)} placeholder="e.g. Platform team" />
            </label>
            <label className={field}>
              Embedding model
              <Input value={form.embed_model ?? ""} onChange={(e) => set("embed_model", e.target.value)} placeholder={d?.embed_model} />
            </label>
            <label className={field}>
              LLM endpoint
              <Input value={form.llm_base_url ?? ""} onChange={(e) => set("llm_base_url", e.target.value)} placeholder={d?.llm_base_url} />
            </label>
            <label className={field}>
              LLM model
              <Input value={form.llm_model ?? ""} onChange={(e) => set("llm_model", e.target.value)} placeholder={d?.llm_model} />
            </label>
          </div>
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
              <Input
                type="number"
                min={1}
                value={form.max_enrich ?? ""}
                onChange={(e) => num("max_enrich", e.target.value)}
                placeholder="no limit"
              />
              <span className="text-xs text-muted-foreground">
                The most-used first. Each summary is one LLM request and sends that code.
              </span>
            </label>
            <label className={field}>
              Most flows to walk through
              <Input
                type="number"
                min={1}
                value={form.max_flows ?? ""}
                onChange={(e) => num("max_flows", e.target.value)}
                placeholder="no limit"
              />
              <span className="text-xs text-muted-foreground">
                Those reaching furthest first. Every flow is still traced without the LLM.
              </span>
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
              <span className="text-xs text-muted-foreground">Lower it if the provider rate-limits; higher indexes faster.</span>
            </label>
            <label className={field}>
              Price per million input tokens (USD)
              <Input
                type="number"
                min={0}
                step="0.01"
                value={form.llm_price_in ?? ""}
                onChange={(e) => price("llm_price_in", e.target.value)}
                placeholder="not set"
              />
              <span className="text-xs text-muted-foreground">
                From your provider's price list; the indexing log then shows what it costs.
              </span>
            </label>
            <label className={field}>
              Price per million output tokens (USD)
              <Input
                type="number"
                min={0}
                step="0.01"
                value={form.llm_price_out ?? ""}
                onChange={(e) => price("llm_price_out", e.target.value)}
                placeholder="not set"
              />
              <span className="text-xs text-muted-foreground">Without prices, the log shows tokens only.</span>
            </label>
            <label className={field}>
              Agents get text as
              <select
                className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
                value={form.format ?? "md"}
                onChange={(e) => set("format", e.target.value === "md" ? undefined : e.target.value)}
              >
                <option value="md">Markdown (reads best)</option>
                <option value="json">JSON</option>
              </select>
              <span className="text-xs text-muted-foreground">Every answer also carries the same data as structured JSON.</span>
            </label>
          </div>
          {admin && (
            <div>
              <Button variant="outline" size="sm" loading={save.isPending} onClick={() => save.mutate()}>
                Save for the team
              </Button>
            </div>
          )}
          <Outcome error={save.error} message={save.data?.message} />
        </fieldset>
      </CardContent>
    </Card>
  );
}
