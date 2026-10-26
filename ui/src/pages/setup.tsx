import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ArrowRight, BookOpen, Check, CircleDashed, Copy, KeyRound } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useAccess } from "@/lib/me";
import { api, type Setup } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

const DOCS = "https://comindly.pages.dev";

/** Questions grouped by what they are for, each with the MCP tool that answers it. */
const ASK: { title: string; items: [string, string][] }[] = [
  {
    title: "Find your way",
    items: [
      ["search", "Where do we retry outgoing HTTP calls?"],
      ["find", "Find the OrderService class."],
      ["zoom", "Show me everything around OrderService.place_order."],
      ["repos", "Which repositories are indexed and how do they depend on each other?"],
      ["suggest", "What can I ask about the payments module?"],
    ],
  },
  {
    title: "Follow it across services",
    items: [
      ["thread", "Trace what happens after place_order is called."],
      ["flow", "Walk me through the checkout flow."],
      ["contracts", "Who calls GET /orders/{id}, and which queue does the order service publish to?"],
    ],
  },
  {
    title: "Change it safely",
    items: [
      ["ripple", "What breaks in other services if I change this function's signature?"],
      ["context_pack", "What should I read before changing OrderService.place_order?"],
      ["preflight", "I am about to change auth.py and errors.py. Anything I should know?"],
      ["guide", "Which conventions should I follow in this repository?"],
    ],
  },
  {
    title: "Team memory",
    items: [
      ["why", "Why is this function written this way? Did anyone try something else?"],
      ["remember", "Remember that the cache TTL of None means entries never expire."],
      ["review", "That constraint about pinning the client library no longer applies; archive it."],
    ],
  },
];

const TERMINAL: [string, string][] = [
  ['comindly search "where do we retry HTTP calls"', "Search by describing what you need"],
  ["comindly explore OrderService.place_order", "A symbol's neighbourhood, blast radius and context"],
  ["comindly contracts --dangling", "Routes and queues missing a side"],
  ["comindly why OrderService", "What the team recorded about a symbol"],
  ["comindly preflight .", "Before you commit"],
  ['comindly remember constraint --on OrderService --title "…"', "Record a constraint"],
];

function useCopy() {
  const [copied, setCopied] = useState<string | null>(null);
  return {
    copied,
    copy: (text: string) => {
      navigator.clipboard?.writeText(text);
      setCopied(text);
      setTimeout(() => setCopied((c) => (c === text ? null : c)), 1500);
    },
  };
}

/** A code block with a copy button; `label` adds a header naming the file or place it goes. */
export function Code({ children, label, shell }: { children: string; label?: string; shell?: boolean }) {
  const { copied, copy } = useCopy();
  const body = (
    <pre className="whitespace-pre-wrap p-3.5 pr-12 font-mono text-xs leading-relaxed text-foreground [overflow-wrap:anywhere]">
      {shell && <span className="select-none text-muted-foreground">$ </span>}
      {children}
    </pre>
  );
  if (!label)
    return (
      <div className="relative rounded-lg border bg-muted/40">
        {body}
        <Button size="icon" variant="ghost" className="absolute right-1.5 top-1.5 size-7" aria-label="Copy" onClick={() => copy(children)}>
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>
    );
  return (
    <div className="overflow-hidden rounded-lg border bg-muted/40">
      <div className="flex h-9 items-center justify-between gap-2 border-b pl-3.5 pr-1.5">
        <span className="truncate font-mono text-[11px] text-muted-foreground">{label}</span>
        <Button size="sm" variant="ghost" className="h-7 gap-1.5 px-2 text-xs" onClick={() => copy(children)}>
          {copied ? <Check /> : <Copy />} {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      {body}
    </div>
  );
}

type State = "done" | "todo" | "optional";

function Step({
  n,
  title,
  state,
  status,
  last,
  children,
}: {
  n: number;
  title: string;
  state: State;
  status?: string;
  last?: boolean;
  children: React.ReactNode;
}) {
  return (
    <li className="relative flex gap-4 pb-8 last:pb-0">
      {!last && <span aria-hidden className="absolute left-[13px] top-8 bottom-1 w-px bg-border" />}
      <span
        className={cn(
          "relative flex size-7 shrink-0 items-center justify-center rounded-full font-mono text-xs",
          state === "done" && "bg-brand text-background",
          state === "todo" && "border border-brand text-brand shadow-[0_0_0_4px_color-mix(in_srgb,var(--color-brand)_12%,transparent)]",
          state === "optional" && "border border-dashed border-muted-foreground/60 text-muted-foreground",
        )}
      >
        {state === "done" ? <Check className="size-3.5" strokeWidth={3} /> : n}
      </span>
      <div className="min-w-0 flex-1 pt-0.5">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h3 className="font-medium">{title}</h3>
          {status && (
            <span className={cn("font-mono text-[11px]", state === "done" ? "text-brand" : "text-muted-foreground")}>{status}</span>
          )}
        </div>
        <div className="mt-3 flex flex-col gap-3 text-sm text-muted-foreground">{children}</div>
      </div>
    </li>
  );
}

const AGENTS = ["claude", "cursor", "codex", "vscode", "other"] as const;
type Agent = (typeof AGENTS)[number];
const AGENT_NAME: Record<Agent, string> = { claude: "Claude Code", cursor: "Cursor", codex: "Codex", vscode: "VS Code", other: "Other" };

function connect(agent: Agent, index: string) {
  const args = ["serve", "--index-dir", index];
  const json = (servers: string, extra: object = {}) =>
    JSON.stringify({ [servers]: { comindly: { ...extra, command: "comindly", args } } }, null, 2);
  switch (agent) {
    case "claude":
      return {
        label: "Terminal · once, for every project",
        shell: true,
        code: `claude mcp add --scope user comindly -- comindly ${args.join(" ")}`,
      };
    case "cursor":
      return { label: "~/.cursor/mcp.json", code: json("mcpServers") };
    case "codex":
      return { label: "~/.codex/config.toml", code: `[mcp_servers.comindly]\ncommand = "comindly"\nargs = ${JSON.stringify(args)}` };
    case "vscode":
      return { label: ".vscode/mcp.json in your workspace", code: json("servers", { type: "stdio" }) };
    default:
      return { label: "Any MCP client that starts a server over stdio", code: json("mcpServers") };
  }
}

function AgentPicker({ agent, setAgent, connected }: { agent: Agent; setAgent: (a: Agent) => void; connected: string[] }) {
  return (
    <div role="tablist" className="flex max-w-full flex-wrap gap-1.5">
      {AGENTS.map((a) => (
        <button
          key={a}
          role="tab"
          aria-selected={agent === a}
          onClick={() => setAgent(a)}
          className={cn(
            "flex h-8 items-center gap-1.5 rounded-md border px-3 text-[13px] transition-colors",
            agent === a ? "border-input bg-accent text-foreground" : "border-transparent text-muted-foreground hover:text-foreground",
          )}
        >
          {connected.includes(a) && <span className="size-1.5 rounded-full bg-brand" aria-label="connected" />}
          {AGENT_NAME[a]}
        </button>
      ))}
    </div>
  );
}

function FirstQuestion() {
  const { copied, copy } = useCopy();
  const tries = [ASK[2].items[0][1], ASK[1].items[1][1], ASK[3].items[0][1]];
  return (
    <div className="flex flex-col gap-2">
      {tries.map((q) => (
        <button
          key={q}
          onClick={() => copy(q)}
          className="group flex items-center justify-between gap-3 rounded-lg border bg-card px-3.5 py-2.5 text-left text-[13px] text-foreground transition-colors hover:border-input"
        >
          <span>“{q}”</span>
          <span className="shrink-0 font-mono text-[11px] text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
            {copied === q ? "copied" : "copy"}
          </span>
        </button>
      ))}
    </div>
  );
}

function Health({ s, server }: { s: Setup | undefined; server: boolean }) {
  if (!s) return <Skeleton className="h-72" />;
  const needed = s.checks.filter((c) => !c.optional);
  const extras = s.checks.filter((c) => c.optional);
  const problems = needed.filter((c) => !c.ok).length;
  const row = (c: Setup["checks"][number]) => (
    <li key={c.message} className="flex items-start gap-2.5 py-1.5">
      {c.ok ? (
        <Check className="mt-0.5 size-3.5 shrink-0 text-brand" strokeWidth={3} />
      ) : c.optional ? (
        <CircleDashed className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      ) : (
        <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-warning" />
      )}
      <span className={cn("min-w-0 text-[13px] [overflow-wrap:anywhere]", !c.ok && "text-muted-foreground")}>
        {sentence(c.message)
          .split("`")
          .map((part, i) => (i % 2 ? <code key={i} className="font-mono text-[12px] text-foreground">{part}</code> : part))}
      </span>
    </li>
  );
  return (
    <Card className="overflow-hidden lg:sticky lg:top-6">
      <div className="border-b px-5 py-4">
        <div className="eyebrow">{server ? "This server" : "This machine"}</div>
        <div className="mt-2 flex items-center gap-2.5">
          <span
            className={cn(
              "size-2 rounded-full",
              problems ? "bg-warning" : "bg-brand shadow-[0_0_0_3px_color-mix(in_srgb,var(--color-brand)_18%,transparent)]",
            )}
          />
          <span className="font-medium">{problems ? `${problems} thing${problems > 1 ? "s" : ""} to fix` : "Ready"}</span>
          <span className="ml-auto font-mono text-[11px] text-muted-foreground">v{s.version}</span>
        </div>
        <div className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground" title={s.index}>
          {s.index}
        </div>
      </div>
      <div className="px-5 py-3">
        <ul>{needed.map(row)}</ul>
        {extras.length > 0 && (
          <>
            <div className="eyebrow mb-1 mt-3">
              Extras · {extras.filter((c) => c.ok).length} of {extras.length}
            </div>
            <ul>{extras.map(row)}</ul>
          </>
        )}
      </div>
      {!server && (
        <div className="border-t px-5 py-3 text-xs text-muted-foreground">
          Same checks in a terminal: <code className="font-mono text-foreground">comindly doctor</code>
        </div>
      )}
    </Card>
  );
}

function LocalSteps({ s }: { s: Setup | undefined }) {
  const connected = s?.agents ?? [];
  const [agent, setAgent] = useState<Agent>((connected[0] as Agent) ?? "claude");
  const index = s?.index || "<index-dir>";
  const how = connect(agent, index);
  const hooks = s?.checks.some((c) => c.ok && c.message.toLowerCase().includes("hooks configured"));
  return (
    <ol>
      <Step n={1} title="Install comindly" state="done" status={s ? `v${s.version} running here` : undefined}>
        <p>For a teammate or another machine, prebuilt for macOS and Linux:</p>
        <Code label="Terminal" shell>
          {"curl -LsSf https://raw.githubusercontent.com/sayef/comindly/main/scripts/install.sh | sh"}
        </Code>
      </Step>
      <Step
        n={2}
        title="Connect your agent"
        state={connected.length ? "done" : "todo"}
        status={connected.length ? `connected: ${connected.map((a) => AGENT_NAME[a as Agent] ?? a).join(", ")}` : "not connected yet"}
      >
        <AgentPicker agent={agent} setAgent={setAgent} connected={connected} />
        <Code label={how.label} shell={how.shell}>
          {how.code}
        </Code>
        {s?.remote && (
          <p>
            The index is in a bucket, so the agent needs the same storage credentials as this app. Hooks read a local mirror that refreshes
            itself.
          </p>
        )}
        <p>Restart the agent. It lists comindly's tools and calls them on its own when a question needs them.</p>
      </Step>
      <Step
        n={3}
        title="Bring team records into every session"
        state={hooks ? "done" : "optional"}
        status={hooks ? "hooks on" : "optional"}
      >
        <p>
          Hooks show the agent what the team recorded without being asked: about the repository when a session starts, and about a file just
          before it is edited. Merge the printed settings into <code className="font-mono text-foreground">~/.claude/settings.json</code>.
        </p>
        <Code label="Terminal" shell>
          {"comindly init-hooks claude"}
        </Code>
        <p className="text-xs">
          Also <code className="font-mono">cursor</code>, <code className="font-mono">gemini</code>,{" "}
          <code className="font-mono">codex</code> and <code className="font-mono">generic</code>.
        </p>
      </Step>
      <Step n={4} title="Ask your first question" state="todo" last>
        <p>In your agent, in plain words. Click one to copy it.</p>
        <FirstQuestion />
      </Step>
    </ol>
  );
}

function ServerSteps() {
  return (
    <ol>
      <Step n={1} title="Create a personal access token" state="todo">
        <p>Your agent talks to this server with it. Nothing to install on your machine.</p>
        <div>
          <Button asChild>
            <Link to="/settings/access">
              <KeyRound /> Your access <ArrowRight />
            </Link>
          </Button>
        </div>
      </Step>
      <Step n={2} title="Add the server to your agent" state="todo">
        <p>The same page shows the command or config for Claude Code, Cursor, Codex and VS Code, with your token filled in.</p>
      </Step>
      <Step n={3} title="Ask your first question" state="todo" last>
        <p>In your agent, in plain words. Click one to copy it.</p>
        <FirstQuestion />
      </Step>
    </ol>
  );
}

function Questions({ s }: { s: Setup | undefined }) {
  const { copied, copy } = useCopy();
  const about = new Map(s?.tools.map((t) => [t.name, t.description]));
  const listed = new Set(ASK.flatMap((g) => g.items.map(([t]) => t)));
  const rest = s?.tools.filter((t) => !listed.has(t.name)) ?? [];
  return (
    <section className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">What your agent can answer</h2>
        <p className="mt-1 text-sm text-muted-foreground">Ask in plain words; the agent picks the tool. Click a question to copy it.</p>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        {ASK.map((g) => (
          <Card key={g.title} className="p-2">
            <div className="eyebrow px-3 pb-1 pt-2">{g.title}</div>
            {g.items.map(([tool, q]) => (
              <button
                key={tool}
                onClick={() => copy(q)}
                title={about.get(tool)}
                className="group flex w-full items-start justify-between gap-3 rounded-md px-3 py-2.5 text-left transition-colors hover:bg-accent"
              >
                <span className="text-[13px]">{q}</span>
                <span className="mt-0.5 shrink-0 font-mono text-[11px] text-muted-foreground group-hover:text-foreground">
                  {copied === q ? "copied" : tool}
                </span>
              </button>
            ))}
          </Card>
        ))}
      </div>
      {rest.length > 0 && (
        <p className="text-xs text-muted-foreground">
          Also:{" "}
          {rest.map((t) => (
            <code key={t.name} className="mr-2 font-mono" title={t.description}>
              {t.name}
            </code>
          ))}
        </p>
      )}
    </section>
  );
}

function Terminal() {
  const { copied, copy } = useCopy();
  return (
    <section className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Without an agent</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          The same answers in a terminal. They use the index in your settings; add <code className="font-mono">--index-dir</code> for
          another.
        </p>
      </div>
      <Card className="divide-y overflow-hidden">
        {TERMINAL.map(([cmd, what]) => (
          <button
            key={cmd}
            onClick={() => copy(cmd)}
            className="group grid w-full gap-1 px-4 py-3 text-left transition-colors hover:bg-accent/60 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:gap-4"
          >
            <code className="min-w-0 truncate font-mono text-xs">
              <span className="select-none text-muted-foreground">$ </span>
              {cmd}
            </code>
            <span className="text-xs text-muted-foreground">{copied === cmd ? <span className="text-brand">Copied</span> : what}</span>
          </button>
        ))}
      </Card>
    </section>
  );
}

export function SetupPage() {
  const setup = useQuery({ queryKey: ["setup"], queryFn: api.setup });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const { server, admin } = useAccess();
  const s = setup.data;

  return (
    <div className="flex flex-col gap-10">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="eyebrow">Get started</div>
          <h1 className="mt-2 text-[28px] font-semibold tracking-[-0.035em]">Give your agent the whole codebase</h1>
          <p className="mt-2 max-w-3xl text-sm text-muted-foreground">
            comindly answers your coding agent's questions across every repository: what calls what, which services talk through which
            routes and queues, what a change can break, and what the team recorded about the code.
          </p>
        </div>
        <Button variant="outline" asChild>
          <a href={DOCS} target="_blank" rel="noreferrer">
            <BookOpen /> Documentation
          </a>
        </Button>
      </div>

      <div className={cn("grid grid-cols-1 items-start gap-6", admin && "lg:grid-cols-[minmax(0,1fr)_340px]")}>
        <Card className="min-w-0 p-4 sm:p-6">{me.data?.mode === "server" ? <ServerSteps /> : <LocalSteps s={s} />}</Card>
        {admin && <Health s={s} server={server} />}
      </div>

      <Questions s={s} />
      {!server && <Terminal />}
    </div>
  );
}
