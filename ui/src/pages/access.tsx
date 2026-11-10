import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ShieldCheck } from "lucide-react";
import { useState } from "react";
import { ConfirmButton } from "@/components/confirm";
import { Outcome } from "@/components/notice";
import { Reveal } from "@/components/reveal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { api, type Role, type SecretState } from "@/lib/api";
import { date } from "@/lib/utils";
import { PageHeader } from "@/components/page-header";
import { ActivityTabs } from "@/pages/activity";
import { Section, SettingsTabs } from "@/pages/settings";
import { Code } from "@/pages/setup";

const select = "h-9 rounded-md border border-input bg-card px-2 text-sm transition-colors duration-100 hover:border-muted-foreground/50";
const ROLES: Record<Role, string> = { viewer: "Viewer", member: "Member", admin: "Admin" };

/** A member's role, changed only after a second, explicit step. */
function RoleSelect({ email, role, pending, onChange }: { email: string; role: Role; pending: boolean; onChange: (r: Role) => void }) {
  const [next, setNext] = useState<Role | null>(null);
  return (
    <div className="flex flex-wrap items-center gap-2">
      <select className={select} value={next ?? role} aria-label={`Role of ${email}`} onChange={(e) => setNext(e.target.value === role ? null : (e.target.value as Role))}>
        {(Object.keys(ROLES) as Role[]).map((r) => (
          <option key={r} value={r}>
            {ROLES[r]}
          </option>
        ))}
      </select>
      {next && (
        <span role="group" aria-label={`Change role of ${email}`} className="inline-flex items-center gap-1">
          <Button size="sm" variant="ghost" onClick={() => setNext(null)}>
            Cancel
          </Button>
          <Button size="sm" variant="outline" loading={pending} onClick={() => (onChange(next), setNext(null))}>
            Make {ROLES[next].toLowerCase()}
          </Button>
        </span>
      )}
    </div>
  );
}

export function MembersPage() {
  const qc = useQueryClient();
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const list = useQuery({ queryKey: ["members"], queryFn: api.members });
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("member");
  const refresh = () => qc.invalidateQueries({ queryKey: ["members"] });
  const invite = useMutation({
    mutationFn: () => api.invite(email, role),
    meta: { inline: true },
    onSuccess: () => (setEmail(""), refresh()),
  });
  const reset = useMutation({
    mutationFn: (e: string) => api.resetCode(e),
    meta: { inline: true },
  });
  const change = useMutation({
    mutationFn: ({ e, r }: { e: string; r: Role }) => api.setRole(e, r),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: (e: string) => api.removeMember(e),
    onSuccess: refresh,
  });
  const revoke = useMutation({
    mutationFn: (e: string) => api.revokeInvite(e),
    onSuccess: refresh,
  });
  if (!list.data) return <Skeleton className="h-96 rounded-lg" />;
  const methods = list.data.sign_in_methods?.length ? list.data.sign_in_methods : [list.data.sign_in];
  const codes = methods.includes("code");
  const emailed = methods.includes("email");

  return (
    <div className="flex max-w-5xl flex-col gap-4">
      <PageHeader title="Members">Who can sign in here and what each person may do: admins change settings, members add to the ledger and conventions, viewers read.</PageHeader>
      <SettingsTabs />
      <Section
        title="Invite someone"
        description={
          emailed
            ? "The invitation is emailed to that address and only works for it, once, within 7 days."
            : codes
              ? "You get an invitation code to pass on. It works once, only with that address, within 7 days; with it they sign in and get their own sign-in code."
              : "They join with this role the first time they sign in with that address, within 7 days."
        }
      >
        <form
          className="flex flex-wrap gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (email.trim()) invite.mutate();
          }}
        >
          <Input
            type="email"
            aria-label="Email address to invite"
            autoComplete="off"
            spellCheck={false}
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="colleague@company.com"
            className="min-w-56 flex-1"
          />
          <select className={select} value={role} aria-label="Role" onChange={(e) => setRole(e.target.value as Role)}>
            <option value="viewer">Viewer</option>
            <option value="member">Member</option>
            <option value="admin">Admin</option>
          </select>
          <Button type="submit" variant="brand" disabled={!email.trim()} loading={invite.isPending}>
            Invite
          </Button>
        </form>
        <p className="mt-2 text-xs text-muted-foreground">
          Viewers read everything. Members also add to the ledger and conventions. Admins also manage people, repositories, settings, secrets and indexing.
        </p>
        <Outcome error={invite.error} />
        {invite.data?.code && (
          <Reveal
            label={invite.data.message}
            value={invite.data.code}
            hint="Copy it now; it is not shown again. They sign in with their email address and this code."
          />
        )}
      </Section>

      <Section
        title="People"
        description="Admins named by the deployment are always admins; change anyone else's role here."
      >
        <Table>
          <thead>
            <Tr>
              <Th>Email</Th>
              <Th>Role</Th>
              <Th>Last seen</Th>
              <Th />
            </Tr>
          </thead>
          <tbody>
            {list.data.members.map((m) => (
              <Tr key={m.email}>
                <Td className="align-middle font-medium">
                  {m.email} {m.email === me.data?.email && <span className="text-muted-foreground">(you)</span>}
                </Td>
                <Td className="align-middle">
                  {m.fixed ? (
                    <Badge variant="outline" title="Set by the deployment (COMINDLY_ADMIN_EMAILS)">
                      <ShieldCheck className="size-3" /> Admin
                    </Badge>
                  ) : m.email === me.data?.email || !me.data ? (
                    <span>{ROLES[m.role]}</span>
                  ) : (
                    <RoleSelect
                      key={m.role}
                      email={m.email}
                      role={m.role}
                      pending={change.isPending && change.variables?.e === m.email}
                      onChange={(r) => change.mutate({ e: m.email, r })}
                    />
                  )}
                </Td>
                <Td className="align-middle text-muted-foreground">{date(m.last_seen)}</Td>
                <Td className="align-middle text-right">
                  <div className="flex justify-end gap-1">
                    {codes && me.data && m.email !== me.data.email && (
                      <ConfirmButton
                        variant="ghost"
                        size="sm"
                        question={`Give ${m.email} a new code? Their current code and sessions stop working.`}
                        confirmLabel="Reset code"
                        loading={reset.isPending && reset.variables === m.email}
                        onConfirm={() => reset.mutate(m.email)}
                      >
                        Reset code
                      </ConfirmButton>
                    )}
                    {!m.fixed && me.data && m.email !== me.data.email && (
                      <ConfirmButton
                        variant="ghost"
                        size="sm"
                        question={`Remove ${m.email}? Their sessions, access tokens, sign-in code and invitations are deleted at once.`}
                        confirmLabel="Remove"
                        loading={remove.isPending && remove.variables === m.email}
                        onConfirm={() => remove.mutate(m.email)}
                      >
                        Remove
                      </ConfirmButton>
                    )}
                  </div>
                </Td>
              </Tr>
            ))}
          </tbody>
        </Table>
        <Outcome error={change.error ?? remove.error ?? reset.error ?? revoke.error} />
        {reset.data?.code && (
          <Reveal
            label={`New sign-in code for ${reset.variables}`}
            value={reset.data.code}
            hint="Copy it now; it is not shown again. Give it to them; the old code no longer works."
          />
        )}
      </Section>

      {list.data.invites.length > 0 && (
        <Section title="Pending invitations">
          <Table>
            <tbody>
              {list.data.invites.map((i) => (
                <Tr key={i.email}>
                  <Td className="align-middle font-medium">{i.email}</Td>
                  <Td className="align-middle capitalize">{i.role}</Td>
                  <Td className="align-middle text-muted-foreground">
                    by {i.created_by}, until {date(i.expires)}
                  </Td>
                  <Td className="align-middle text-right">
                    <ConfirmButton
                      variant="ghost"
                      size="sm"
                      question={`Withdraw the invitation for ${i.email}?`}
                      confirmLabel="Withdraw"
                      loading={revoke.isPending && revoke.variables === i.email}
                      onConfirm={() => revoke.mutate(i.email)}
                    >
                      Withdraw
                    </ConfirmButton>
                  </Td>
                </Tr>
              ))}
            </tbody>
          </Table>
        </Section>
      )}
    </div>
  );
}

function SecretRow({ s, canStore }: { s: SecretState; canStore: boolean }) {
  const qc = useQueryClient();
  const [value, setValue] = useState("");
  const save = useMutation({
    mutationFn: (v: string) => api.setSecret(s.name, v),
    meta: { inline: true },
    onSuccess: () => (setValue(""), qc.invalidateQueries({ queryKey: ["secrets"] })),
  });
  const removing = save.isPending && save.variables === "";
  return (
    <div className="flex flex-col gap-2 border-b py-3 last:border-0">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-semibold">{s.label}</span>
        {s.source === "deployment" && <Badge variant="outline">From the deployment</Badge>}
        {s.source === "stored" && (
          <Badge variant="outline">
            <span className="font-mono">{s.hint}</span>
          </Badge>
        )}
        {s.source === "none" && <span className="text-xs text-muted-foreground">Not set</span>}
        {s.source === "stored" && (
          <span className="text-xs text-muted-foreground">
            by {s.updated_by}, {date(s.updated)}
            {s.replaces_deployment && ", used instead of the deployment's"}
          </span>
        )}
      </div>
      {canStore && (
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (value.trim()) save.mutate(value);
          }}
        >
          <Input
            type="password"
            autoComplete="off"
            spellCheck={false}
            aria-label={s.label}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={
              s.source === "stored" ? "Replace with a new value…" : s.source === "deployment" ? "Paste one to use instead of the deployment's…" : "Paste the token or key…"
            }
            className="flex-1"
          />
          <Button type="submit" variant="outline" size="sm" disabled={!value.trim()} loading={save.isPending && !removing}>
            Save
          </Button>
          {s.source === "stored" && (
            <ConfirmButton
              type="button"
              variant="ghost"
              size="sm"
              question={
                s.replaces_deployment
                  ? `Remove the stored value for ${s.label}? The deployment's is used again.`
                  : `Remove the stored value for ${s.label}? The server stops using it.`
              }
              confirmLabel="Remove"
              loading={removing}
              onConfirm={() => save.mutate("")}
            >
              Remove
            </ConfirmButton>
          )}
        </form>
      )}
      <Outcome error={save.error} />
    </div>
  );
}

export function IntegrationsPage() {
  const list = useQuery({ queryKey: ["secrets"], queryFn: api.secrets });
  const [host, setHost] = useState("");
  const [extra, setExtra] = useState<string[]>([]);
  if (!list.data) return <Skeleton className="h-96 rounded-lg" />;
  const names = new Set(list.data.secrets.map((s) => s.name));
  const rows = [
    ...list.data.secrets,
    ...extra
      .filter((h) => !names.has(`git:${h}`))
      .map((h): SecretState => ({
        name: `git:${h}`,
        label: `Git token for ${h}`,
        source: "none",
      })),
  ];

  return (
    <div className="flex max-w-5xl flex-col gap-4">
      <PageHeader title="Integrations">Tokens and keys the server uses to clone repositories and call the LLM. Values are encrypted and never shown again.</PageHeader>
      <SettingsTabs />
      <Section
        title="Tokens and keys"
        description="What the server uses to list and clone repositories and to call the LLM. Values never come back to the browser. One stored here is used instead of one the deployment provides."
      >
        {!list.data.can_store && (
          <p className="text-xs text-muted-foreground">
            To store tokens here, the deployment must set <code className="font-mono">COMINDLY_SECRET_KEY</code>.
          </p>
        )}
        <div>
          {rows.map((s) => (
            <SecretRow key={s.name} s={s} canStore={list.data.can_store} />
          ))}
        </div>
        {list.data.can_store && (
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const h = host
                .trim()
                .toLowerCase()
                .replace(/^https?:\/\//, "")
                .replace(/\/.*$/, "");
              if (h) setExtra((x) => [...x, h]);
              setHost("");
            }}
          >
            <Input
              aria-label="Git host"
              autoComplete="off"
              spellCheck={false}
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="Another host, e.g. gitlab.company.com"
              className="flex-1"
            />
            <Button type="submit" variant="outline" size="sm" disabled={!host.trim()}>
              Add host
            </Button>
          </form>
        )}
        <p className="text-xs text-muted-foreground">
          Read-only is enough: a GitLab group access token with <code className="font-mono">read_repository</code> and{" "}
          <code className="font-mono">read_api</code>, or a fine-grained GitHub token with Contents and Metadata read.
        </p>
      </Section>
    </div>
  );
}

/** Your own sign-in code (in code mode) and your tokens for agents. */
export function AccessPage() {
  const qc = useQueryClient();
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const newCode = useMutation({ mutationFn: api.resetOwnCode, meta: { inline: true } });
  const list = useQuery({ queryKey: ["tokens"], queryFn: api.tokens });
  const [name, setName] = useState("");
  const [days, setDays] = useState("90");
  const create = useMutation({
    mutationFn: () => api.createToken(name, days === "never" ? null : Number(days)),
    meta: { inline: true },
    onSuccess: () => (setName(""), qc.invalidateQueries({ queryKey: ["tokens"] })),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeToken(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });
  if (!list.data) return <Skeleton className="h-64 rounded-lg" />;

  return (
    <div className="flex max-w-5xl flex-col gap-4">
      <PageHeader title="Your access">How you sign in, and the personal access tokens your agents and command line use.</PageHeader>
      {(me.data?.sign_in_methods ?? [me.data?.sign_in]).includes("code") && (
        <Section
          title="Your sign-in code"
          description="You sign in with your email address and this code. A new one replaces it and signs you out everywhere else."
        >
          <div>
            <ConfirmButton
              variant="outline"
              size="sm"
              loading={newCode.isPending}
              question="Make a new sign-in code? The current one stops working."
              confirmLabel="Make a new code"
              onConfirm={() => newCode.mutate()}
            >
              Make a new sign-in code
            </ConfirmButton>
          </div>
          <Outcome error={newCode.error} />
          {newCode.data?.code && (
            <Reveal label="Your new sign-in code" value={newCode.data.code} hint="Copy it now; it is not shown again." />
          )}
        </Section>
      )}
      <Section
        title="Personal access tokens"
        description="Agents and scripts use comindly as you with a token, sent as Authorization: Bearer …. They stop working if you are removed from the org."
      >
        <form
          className="flex flex-wrap gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) create.mutate();
          }}
        >
          <Input
            aria-label="Token name"
            autoComplete="off"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name, e.g. Claude Code on my laptop…"
            className="min-w-48 flex-1 basis-60"
          />
          <select className={select} value={days} onChange={(e) => setDays(e.target.value)} aria-label="Expires">
            <option value="30">30 days</option>
            <option value="90">90 days</option>
            <option value="365">1 year</option>
            <option value="never">No expiry</option>
          </select>
          <Button type="submit" variant="brand" disabled={!name.trim()} loading={create.isPending}>
            Create token
          </Button>
        </form>
        <Outcome error={create.error ?? revoke.error} />
        {create.data?.token && <Reveal label="Your new token" value={create.data.token} hint="Copy it now; it is not shown again." />}
        <Connect url={me.data?.public_url ?? window.location.origin} gateway={me.data?.gateway_url} token={create.data?.token} />
        {list.data.tokens.length > 0 && (
          <Table>
            <thead>
              <Tr>
                <Th>Name</Th>
                <Th>Created</Th>
                <Th>Last used</Th>
                <Th>Expires</Th>
                <Th />
              </Tr>
            </thead>
            <tbody>
              {list.data.tokens.map((t) => (
                <Tr key={t.id}>
                  <Td className="align-middle font-medium">{t.name}</Td>
                  <Td className="align-middle text-muted-foreground">{date(t.created)}</Td>
                  <Td className="align-middle text-muted-foreground">{date(t.last_used)}</Td>
                  <Td className="align-middle text-muted-foreground">
                    {t.expired ? <Badge variant="outline">Expired</Badge> : t.expires ? date(t.expires) : "Never"}
                  </Td>
                  <Td className="align-middle text-right">
                    <ConfirmButton
                      variant="ghost"
                      size="sm"
                      question={`Revoke ${t.name}? Agents using it lose access.`}
                      confirmLabel="Revoke"
                      loading={revoke.isPending && revoke.variables === t.id}
                      onConfirm={() => revoke.mutate(t.id)}
                    >
                      Revoke
                    </ConfirmButton>
                  </Td>
                </Tr>
              ))}
            </tbody>
          </Table>
        )}
      </Section>
    </div>
  );
}

/** Who changed what in members, tokens, secrets and repositories, newest first. */
export function AuditPage() {
  const log = useInfiniteQuery({
    queryKey: ["audit"],
    queryFn: ({ pageParam }) => api.audit(pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next,
  });
  if (!log.data) return <Skeleton className="h-96 rounded-lg" />;
  const entries = log.data.pages.flatMap((p) => p.entries);
  const when = (t: string) => (t ? new Date(t).toLocaleString() : "—");

  return (
    <div className="flex max-w-5xl flex-col gap-4">
      <PageHeader title="Activity">Who changed what on this server, newest first.</PageHeader>
      <ActivityTabs />
      <Section title="Events" description="Sign-ins, invitations, role changes, tokens, secrets and repositories. Token values and codes are never recorded.">
        {entries.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nothing recorded yet.</p>
        ) : (
          <Table>
            <thead>
              <Tr>
                <Th>When</Th>
                <Th>Who</Th>
                <Th>What</Th>
                <Th>Detail</Th>
              </Tr>
            </thead>
            <tbody>
              {entries.map((e, i) => (
                <Tr key={`${e.at}-${i}`}>
                  <Td className="align-middle whitespace-nowrap text-muted-foreground">{when(e.at)}</Td>
                  <Td className="align-middle font-medium">{e.actor}</Td>
                  <Td className="align-middle">{e.action}</Td>
                  <Td className="align-middle text-muted-foreground">{e.detail}</Td>
                </Tr>
              ))}
            </tbody>
          </Table>
        )}
        {log.hasNextPage && (
          <div>
            <Button variant="outline" size="sm" loading={log.isFetchingNextPage} onClick={() => log.fetchNextPage()}>
              Load more
            </Button>
          </div>
        )}
        <Outcome error={log.error} />
      </Section>
    </div>
  );
}

const AGENTS = { claude: "Claude Code", cursor: "Cursor", codex: "Codex", vscode: "VS Code", app: "Claude app" } as const;
type Agent = keyof typeof AGENTS;

/** How an agent reaches this server's tools: through the gateway with the organisation's sign-in, or
 * over HTTP with a personal access token. */
function Connect({ url, gateway, token }: { url: string; gateway?: string; token?: string }) {
  const [agent, setAgent] = useState<Agent>("claude");
  const [via, setVia] = useState<"gateway" | "token">(gateway ? "gateway" : "token");
  const signIn = via === "gateway" && !!gateway;
  const mcp = signIn ? gateway : `${url}/mcp`;
  const t = token ?? "<token>";
  const auth = signIn ? {} : { headers: { Authorization: `Bearer ${t}` } };
  const how: Record<Agent, [string, string]> = {
    claude: [
      "Run once in a terminal" + (signIn ? ", then /mcp in Claude Code to sign in:" : ":"),
      `claude mcp add --transport http --scope user comindly ${mcp}` + (signIn ? "" : ` --header "Authorization: Bearer ${t}"`),
    ],
    cursor: ["Add to ~/.cursor/mcp.json:", JSON.stringify({ mcpServers: { comindly: { url: mcp, ...auth } } }, null, 2)],
    codex: signIn
      ? ["Add to ~/.codex/config.toml, then run codex mcp login comindly to sign in:", `[mcp_servers.comindly]\nurl = "${mcp}"`]
      : [
          "Add to ~/.codex/config.toml, and set COMINDLY_TOKEN to the token in your shell:",
          `[mcp_servers.comindly]\nurl = "${mcp}"\nbearer_token_env_var = "COMINDLY_TOKEN"`,
        ],
    vscode: [
      "Add to .vscode/mcp.json in your workspace:",
      JSON.stringify({ servers: { comindly: { type: "http", url: mcp, ...auth } } }, null, 2),
    ],
    app: ["In the Claude desktop app or on claude.ai: Settings, Connectors, Add custom connector, with this address:", mcp],
  };
  const agents = (Object.keys(AGENTS) as Agent[]).filter((a) => a !== "app" || signIn);
  const shown = agents.includes(agent) ? agent : "claude";
  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="text-sm font-medium">Connect an agent</div>
      {gateway && (
        <div role="group" aria-label="How" className="flex flex-wrap gap-1">
          <Button variant={signIn ? "outline" : "ghost"} size="sm" aria-pressed={signIn} onClick={() => setVia("gateway")}>
            Sign in, no token
          </Button>
          <Button variant={signIn ? "ghost" : "outline"} size="sm" aria-pressed={!signIn} onClick={() => setVia("token")}>
            With a token
          </Button>
        </div>
      )}
      <div role="group" aria-label="Agent" className="flex flex-wrap gap-1">
        {agents.map((a) => (
          <Button key={a} variant={a === shown ? "outline" : "ghost"} size="sm" aria-pressed={a === shown} onClick={() => setAgent(a)}>
            {AGENTS[a]}
          </Button>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">{how[shown][0]}</p>
      <Code>{how[shown][1]}</Code>
      <p className="text-xs text-muted-foreground">
        {signIn
          ? "The agent signs you in with your organisation's account, from anywhere; it needs no token and no VPN. "
          : ""}
        The agent then lists comindly's tools; ledger records it writes are attributed to you.
      </p>
    </div>
  );
}
