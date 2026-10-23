import { useMutation } from "@tanstack/react-query";
import { KeyRound, Mail } from "lucide-react";
import { useState } from "react";
import { Outcome } from "@/components/notice";
import { Reveal } from "@/components/reveal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, type Me } from "@/lib/api";

const field = "flex flex-col gap-1 text-xs font-medium";
const AFTER = "comindly:after-sign-in";

/** Back where sign-in interrupted, else home. */
const home = () => {
  let to = "/";
  try {
    to = localStorage.getItem(AFTER) ?? "/";
    localStorage.removeItem(AFTER);
  } catch {
    // storage blocked: home it is
  }
  window.location.replace(/^\/(?![/\\])/.test(to) ? to : "/");
};

export function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-full items-center justify-center bg-sidebar p-6">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex items-center justify-center gap-2.5">
          <img src="/logo.svg" alt="" className="size-8 dark:hidden" />
          <img src="/logo-dark.svg" alt="" className="hidden size-8 dark:block" />
          <span className="text-lg font-semibold tracking-tight">comindly</span>
        </div>
        <div className="rounded-xl border bg-card p-6 shadow-sm">{children}</div>
      </div>
    </div>
  );
}

/** Opened from an emailed link: exchanging it is a click, so mail scanners cannot spend it. */
function Confirm({ token }: { token: string }) {
  const redeem = useMutation({ mutationFn: () => api.redeem(token), meta: { inline: true, quiet: true }, onSuccess: home });
  return (
    <Shell>
      <h1 className="text-base font-semibold">Continue to comindly</h1>
      <p className="mt-1 text-sm text-muted-foreground">This link signs you in on this browser. It works once.</p>
      <Button variant="brand" className="mt-5 w-full" loading={redeem.isPending} onClick={() => redeem.mutate()}>
        Sign in
      </Button>
      <div className="mt-3">
        <Outcome error={redeem.error} />
      </div>
    </Shell>
  );
}

function ByEmail() {
  const [email, setEmail] = useState("");
  const send = useMutation({ mutationFn: () => api.requestLink(email), meta: { inline: true, quiet: true } });
  if (send.data)
    return (
      <div className="mt-4 flex flex-col items-center gap-2 py-4 text-center">
        <Mail className="size-6 text-brand" />
        <p className="text-sm">{send.data.message}.</p>
        <p className="text-xs text-muted-foreground">The link works once, for 15 minutes.</p>
        <Button variant="ghost" size="sm" onClick={() => send.reset()}>
          Use another address
        </Button>
      </div>
    );
  return (
    <form
      className="mt-4 flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (email.trim()) send.mutate();
      }}
    >
      <label className={field}>
        Work email
        <Input
          type="email"
          autoComplete="email"
          autoFocus
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@company.com"
        />
      </label>
      <Button type="submit" variant="brand" disabled={!email.trim()} loading={send.isPending}>
        Email me a sign-in link
      </Button>
      <Outcome error={send.error} />
    </form>
  );
}

function ByCode() {
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const signIn = useMutation({
    mutationFn: () => api.signInCode(email, code),
    meta: { inline: true, quiet: true },
    onSuccess: (r) => !r.code && home(),
  });
  if (signIn.data?.code)
    return (
      <div className="mt-4 flex flex-col gap-4">
        <p className="text-sm">Welcome. From now on, sign in with your email address and this code.</p>
        <Reveal
          label="Your sign-in code"
          value={signIn.data.code}
          hint="Save it in your password manager. It is not shown again; you can make a new one under Settings → Your access."
        />
        <Button variant="brand" onClick={home}>
          I have saved it
        </Button>
      </div>
    );
  return (
    <form
      className="mt-4 flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (email.trim() && code.trim()) signIn.mutate();
      }}
    >
      <label className={field}>
        Work email
        <Input
          type="email"
          autoComplete="username"
          autoFocus
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@company.com"
        />
      </label>
      <label className={field}>
        Code
        <Input
          type="password"
          autoComplete="current-password"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="XXXX-XXXX-XXXX-XXXX-XXXX"
          className="font-mono"
        />
      </label>
      <Button type="submit" variant="brand" disabled={!email.trim() || !code.trim()} loading={signIn.isPending}>
        Sign in
      </Button>
      <p className="text-xs text-muted-foreground">
        Your sign-in code, or the invitation code an admin gave you. Lost it? Ask an admin for a new one.
      </p>
      <Outcome error={signIn.error} />
    </form>
  );
}

function AdminToken() {
  const [open, setOpen] = useState(false);
  const [secret, setSecret] = useState("");
  const redeem = useMutation({ mutationFn: () => api.redeem(secret), meta: { inline: true, quiet: true }, onSuccess: home });
  if (!open)
    return (
      <button className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground" onClick={() => setOpen(true)}>
        <KeyRound className="size-3.5" /> Sign in with the deployment's admin token
      </button>
    );
  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (secret.trim()) redeem.mutate();
      }}
    >
      <label className={field}>
        Admin token
        <Input type="password" autoComplete="off" value={secret} onChange={(e) => setSecret(e.target.value)} />
      </label>
      <Button type="submit" variant="outline" size="sm" disabled={!secret.trim()} loading={redeem.isPending}>
        Sign in as admin
      </Button>
      <Outcome error={redeem.error} />
    </form>
  );
}

export function SignInPage({ me }: { me: Me }) {
  const token = new URLSearchParams(window.location.search).get("t");
  if (window.location.pathname !== "/signin") {
    try {
      localStorage.setItem(AFTER, window.location.pathname + window.location.search);
    } catch {
      // storage blocked: sign-in returns home
    }
  }
  if (token && me.sign_in === "email") return <Confirm token={token} />;
  return (
    <Shell>
      <h1 className="text-base font-semibold">Sign in</h1>
      {me.sign_in === "email" ? <ByEmail /> : <ByCode />}
      <div className="mt-5 border-t pt-4">
        <AdminToken />
      </div>
    </Shell>
  );
}
