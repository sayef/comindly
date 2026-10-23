import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ShieldCheck } from "lucide-react";
import { useState } from "react";
import { Outcome } from "@/components/notice";
import { Reveal } from "@/components/reveal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { api, type Role, type SecretState } from "@/lib/api";
import { Section } from "@/pages/settings";
import { Code } from "@/pages/setup";

const select = "h-9 rounded-md border bg-card px-2 text-sm";
const day = (t?: string | null) => (t ? new Date(t).toLocaleDateString() : "—");

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
  if (!list.data) return <Skeleton className="h-96 rounded-xl" />;
  const codes = list.data.sign_in === "code";

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <Section
        title="Invite someone"
        description={
          codes
            ? "You get an invitation code to pass on. It works once, only with that address, within 7 days; with it they sign in and get their own sign-in code."
            : "The invitation is emailed to that address and only works for it, once, within 7 days."
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
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="colleague@company.com"
            className="min-w-56 flex-1"
          />
          <select className={select} value={role} onChange={(e) => setRole(e.target.value as Role)}>
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
        title="Members"
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
                  ) : (
                    <select className={select} value={m.role} onChange={(e) => change.mutate({ e: m.email, r: e.target.value as Role })}>
                      <option value="viewer">Viewer</option>
                      <option value="member">Member</option>
                      <option value="admin">Admin</option>
                    </select>
                  )}
                </Td>
                <Td className="align-middle text-muted-foreground">{day(m.last_seen)}</Td>
                <Td className="align-middle text-right">
                  <div className="flex justify-end gap-1">
                    {codes && m.email !== me.data?.email && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() =>
                          confirm(`Give ${m.email} a new sign-in code? Their current code and sessions stop working.`) &&
                          reset.mutate(m.email)
                        }
                      >
                        Reset code
                      </Button>
                    )}
                    {!m.fixed && m.email !== me.data?.email && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => confirm(`Remove ${m.email}? Their sessions and tokens stop working.`) && remove.mutate(m.email)}
                      >
                        Remove
                      </Button>
                    )}
                  </div>
                </Td>
              </Tr>
            ))}
          </tbody>
        </Table>
        <Outcome error={change.error ?? remove.error ?? reset.error} />
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
                    by {i.created_by}, until {day(i.expires)}
                  </Td>
                  <Td className="align-middle text-right">
                    <Button variant="ghost" size="sm" onClick={() => revoke.mutate(i.email)}>
                      Withdraw
                    </Button>
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
  return (
    <div className="flex flex-col gap-2 border-b py-3 last:border-0">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium">{s.label}</span>
        {s.source === "deployment" && <Badge variant="outline">From the deployment</Badge>}
        {s.source === "stored" && (
          <Badge variant="outline" className="font-mono">
            {s.hint}
          </Badge>
        )}
        {s.source === "none" && <span className="text-xs text-muted-foreground">Not set</span>}
        {s.source === "stored" && (
          <span className="text-xs text-muted-foreground">
            by {s.updated_by}, {day(s.updated)}
          </span>
        )}
      </div>
      {s.source !== "deployment" && canStore && (
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
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={s.source === "stored" ? "Replace with a new value" : "Paste the token or key"}
            className="flex-1"
          />
          <Button type="submit" variant="outline" size="sm" disabled={!value.trim()} loading={save.isPending && !!value}>
            Save
          </Button>
          {s.source === "stored" && (
            <Button type="button" variant="ghost" size="sm" onClick={() => save.mutate("")}>
              Remove
            </Button>
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
  if (!list.data) return <Skeleton className="h-96 rounded-xl" />;
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
    <div className="flex max-w-3xl flex-col gap-5">
      <Section
        title="Tokens and keys"
        description="What the server uses to list and clone repositories and to call the LLM. Values never come back to the browser. Ones the deployment provides win and are changed where it keeps its secrets."
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
  const create = useMutation({
    mutationFn: () => api.createToken(name),
    meta: { inline: true },
    onSuccess: () => (setName(""), qc.invalidateQueries({ queryKey: ["tokens"] })),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeToken(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });
  if (!list.data) return <Skeleton className="h-64 rounded-xl" />;

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      {me.data?.sign_in === "code" && (
        <Section
          title="Your sign-in code"
          description="You sign in with your email address and this code. A new one replaces it and signs you out everywhere else."
        >
          <div>
            <Button
              variant="outline"
              size="sm"
              loading={newCode.isPending}
              onClick={() => confirm("Make a new sign-in code? The current one stops working.") && newCode.mutate()}
            >
              Make a new sign-in code
            </Button>
          </div>
          <Outcome error={newCode.error} />
          {newCode.data?.code && (
            <Reveal label="Your new sign-in code" value={newCode.data.code} hint="Copy it now; it is not shown again." />
          )}
        </Section>
      )}
      <Section
        title="Personal access tokens"
        description="Agents and scripts use comindly as you with a token, sent as Authorization: Bearer …. They stop working if you are removed from the team."
      >
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) create.mutate();
          }}
        >
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name, e.g. Claude Code on my laptop"
            className="flex-1"
          />
          <Button type="submit" variant="brand" disabled={!name.trim()} loading={create.isPending}>
            Create token
          </Button>
        </form>
        <Outcome error={create.error} />
        {create.data?.token && <Reveal label="Your new token" value={create.data.token} hint="Copy it now; it is not shown again." />}
        <Connect url={me.data?.public_url ?? window.location.origin} token={create.data?.token} />
        {list.data.tokens.length > 0 && (
          <Table>
            <thead>
              <Tr>
                <Th>Name</Th>
                <Th>Created</Th>
                <Th>Last used</Th>
                <Th />
              </Tr>
            </thead>
            <tbody>
              {list.data.tokens.map((t) => (
                <Tr key={t.id}>
                  <Td className="align-middle font-medium">{t.name}</Td>
                  <Td className="align-middle text-muted-foreground">{day(t.created)}</Td>
                  <Td className="align-middle text-muted-foreground">{day(t.last_used)}</Td>
                  <Td className="align-middle text-right">
                    <Button variant="ghost" size="sm" onClick={() => revoke.mutate(t.id)}>
                      Revoke
                    </Button>
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

const AGENTS = { claude: "Claude Code", cursor: "Cursor", codex: "Codex", vscode: "VS Code" } as const;

/** How an agent reaches this server's tools over HTTP with a personal access token. */
function Connect({ url, token }: { url: string; token?: string }) {
  const [agent, setAgent] = useState<keyof typeof AGENTS>("claude");
  const mcp = `${url}/mcp`;
  const t = token ?? "<token>";
  const how = {
    claude: [
      "Run once in a terminal:",
      `claude mcp add --transport http --scope user comindly ${mcp} --header "Authorization: Bearer ${t}"`,
    ],
    cursor: [
      "Add to ~/.cursor/mcp.json:",
      JSON.stringify({ mcpServers: { comindly: { url: mcp, headers: { Authorization: `Bearer ${t}` } } } }, null, 2),
    ],
    codex: [
      "Add to ~/.codex/config.toml, and set COMINDLY_TOKEN to the token in your shell:",
      `[mcp_servers.comindly]\nurl = "${mcp}"\nbearer_token_env_var = "COMINDLY_TOKEN"`,
    ],
    vscode: [
      "Add to .vscode/mcp.json in your workspace:",
      JSON.stringify({ servers: { comindly: { type: "http", url: mcp, headers: { Authorization: `Bearer ${t}` } } } }, null, 2),
    ],
  }[agent];
  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="text-sm font-medium">Connect an agent</div>
      <div className="flex flex-wrap gap-1">
        {(Object.keys(AGENTS) as (keyof typeof AGENTS)[]).map((a) => (
          <Button key={a} variant={a === agent ? "outline" : "ghost"} size="sm" onClick={() => setAgent(a)}>
            {AGENTS[a]}
          </Button>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">{how[0]}</p>
      <Code>{how[1]}</Code>
      <p className="text-xs text-muted-foreground">The agent then lists comindly's tools; ledger records it writes are attributed to you.</p>
    </div>
  );
}
