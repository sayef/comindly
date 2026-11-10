import { useMutation } from "@tanstack/react-query";
import { Building2, KeyRound, Mail } from "lucide-react";
import { useEffect, useState } from "react";
import { PanelGraph } from "@/components/backdrop";
import { Notice, Outcome } from "@/components/notice";
import { Reveal } from "@/components/reveal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, type Me, type SignIn } from "@/lib/api";

const field = "flex flex-col gap-1.5 text-sm font-semibold";
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

function Logo() {
  return (
    <a href="/" className="flex w-fit items-center gap-3 rounded-md">
      <img src="/logo.svg" alt="" className="size-9 xl:size-11 dark:hidden" />
      <img src="/logo-dark.svg" alt="" className="hidden size-9 xl:size-11 dark:block" />
      <span className="text-[22px] font-semibold tracking-[-0.015em] xl:text-[26px]" translate="no">
        comindly
      </span>
    </a>
  );
}

const LINES = [
  "Know what breaks before you change it.",
  "Change one function, see every caller.",
  "One graph across every repository.",
  "What the team learned, where agents look.",
  "One binary. Your storage.",
];

/** Types each line, holds it, erases it and moves to the next; still with reduced motion. */
function Typed() {
  const [line, setLine] = useState(0);
  const [shown, setShown] = useState(0);
  const [erasing, setErasing] = useState(false);
  const still = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  useEffect(() => {
    if (still) return;
    const text = LINES[line];
    const done = shown === text.length;
    const gone = shown === 0;
    const wait = erasing ? (gone ? 400 : 22) : done ? 2600 : 48;
    const timer = setTimeout(() => {
      if (!erasing && done) setErasing(true);
      else if (erasing && gone) {
        setErasing(false);
        setLine((line + 1) % LINES.length);
      } else setShown(shown + (erasing ? -1 : 1));
    }, wait);
    return () => clearTimeout(timer);
  }, [line, shown, erasing, still]);
  const text = still ? LINES[0] : LINES[line].slice(0, shown);
  return (
    <p className="min-h-[2.5em] text-5xl leading-[1.15] font-semibold tracking-[-0.03em] text-balance xl:text-6xl 2xl:text-7xl">
      <span className="sr-only">{LINES[0]}</span>
      <span aria-hidden="true">
        {text}
        <span className="caret ml-0.5 inline-block h-[0.9em] w-[3px] translate-y-[0.12em] rounded-full bg-brand" />
      </span>
    </p>
  );
}

/** Sign-in and approval pages: what comindly does on the left from wide screens up, the form on the right. */
export function Shell({ children, footer }: { children: React.ReactNode; footer?: React.ReactNode }) {
  return (
    <div className="grid min-h-dvh bg-background lg:grid-cols-[1.1fr_1fr]">
      <aside className="relative isolate hidden overflow-hidden bg-muted lg:block">
        <PanelGraph />
        <div className="relative flex h-full flex-col justify-between p-10 xl:p-14">
          <Logo />
          <div className="flex max-w-2xl flex-col gap-6">
            <p className="text-lg text-muted-foreground xl:text-xl">Your team's engineering memory for coding agents</p>
            <Typed />
          </div>
          <p className="text-sm text-muted-foreground">Contracts, flows, conventions and decisions across every repository, over MCP.</p>
        </div>
      </aside>
      <div className="flex flex-col gap-6 p-6 md:p-10">
        <div className="flex justify-center lg:hidden">
          <Logo />
        </div>
        <main className="flex flex-1 items-center justify-center">
          <div className="w-full max-w-sm xl:max-w-md">{children}</div>
        </main>
        {footer && <div className="flex justify-center">{footer}</div>}
      </div>
    </div>
  );
}

function Heading({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h1 className="text-3xl font-semibold tracking-[-0.025em] text-balance">{title}</h1>
      {children && <p className="text-base text-muted-foreground">{children}</p>}
    </div>
  );
}

/** Opened from an emailed link: exchanging it is a click, so mail scanners cannot spend it. */
function Confirm({ token }: { token: string }) {
  const redeem = useMutation({ mutationFn: () => api.redeem(token), meta: { inline: true, quiet: true }, onSuccess: home });
  return (
    <Shell>
      <Heading title="Continue to comindly">This link signs you in on this browser. It works once.</Heading>
      <Button variant="brand" className="mt-6 w-full" loading={redeem.isPending} onClick={() => redeem.mutate()}>
        Sign in
      </Button>
      <div className="mt-3">
        <Outcome error={redeem.error} />
      </div>
    </Shell>
  );
}

function BrandLogo({ brand }: { brand?: Me["oidc_brand"] }) {
  if (brand === "microsoft")
    return (
      <svg viewBox="0 0 21 21" className="size-4" aria-hidden="true">
        <rect x="1" y="1" width="9" height="9" fill="#f25022" />
        <rect x="11" y="1" width="9" height="9" fill="#7fba00" />
        <rect x="1" y="11" width="9" height="9" fill="#00a4ef" />
        <rect x="11" y="11" width="9" height="9" fill="#ffb900" />
      </svg>
    );
  if (brand === "google")
    return (
      <svg viewBox="0 0 48 48" className="size-4" aria-hidden="true">
        <path fill="#ffc107" d="M43.6 20.1H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 12.9 4 4 12.9 4 24s8.9 20 20 20 20-8.9 20-20c0-1.3-.1-2.6-.4-3.9z" />
        <path fill="#ff3d00" d="m6.3 14.7 6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.8 1.2 7.9 3.1l5.7-5.7C34 6.1 29.3 4 24 4 16.3 4 9.7 8.3 6.3 14.7z" />
        <path fill="#4caf50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2c-2 1.5-4.5 2.4-7.2 2.4-5.2 0-9.6-3.3-11.3-8l-6.5 5C9.5 39.6 16.2 44 24 44z" />
        <path fill="#1976d2" d="M43.6 20.1H42V20H24v8h11.3c-.8 2.2-2.2 4.2-4.1 5.6l6.2 5.2C37 39.2 44 34 44 24c0-1.3-.1-2.6-.4-3.9z" />
      </svg>
    );
  return <Building2 className="size-4" />;
}

/** Leaves for the company's sign-in provider, which sends the browser back signed in; a known
 * provider's button is plain with its logo, as those providers ask. */
function ByProvider({ label, brand }: { label: string; brand?: Me["oidc_brand"] }) {
  return (
    <Button asChild variant={brand ? "outline" : "brand"} size="lg" className="w-full text-[15px]">
      <a href="/auth/oidc/start">
        <BrandLogo brand={brand} />
        Continue with {label}
      </a>
    </Button>
  );
}

function Divider({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3 text-xs text-muted-foreground">
      <span className="h-px flex-1 bg-border" />
      {children}
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

/** An emailed link, a code, or both: one address field, and the code field when it is the way chosen. */
function ByAddress({ email: byEmail, code: byCode, primary }: { email: boolean; code: boolean; primary: boolean }) {
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [withCode, setWithCode] = useState(!byEmail);
  const send = useMutation({ mutationFn: () => api.requestLink(email), meta: { inline: true, quiet: true } });
  const signIn = useMutation({
    mutationFn: () => api.signInCode(email, code),
    meta: { inline: true, quiet: true },
    onSuccess: (r) => !r.code && home(),
  });
  if (signIn.data?.code)
    return (
      <div className="flex flex-col gap-4">
        <p className="text-sm">Welcome. You can also sign in with your email address and this code.</p>
        <Reveal
          label="Your sign-in code"
          value={signIn.data.code}
          hint="Save it in your password manager. It is not shown again; you can make a new one under Your access."
        />
        <Button variant="brand" onClick={home}>
          I have saved it
        </Button>
      </div>
    );
  if (send.data)
    return (
      <div className="flex flex-col items-center gap-2 rounded-lg bg-muted p-5 text-center">
        <Mail className="size-6 text-brand" />
        <p className="text-sm">{send.data.message}.</p>
        <p className="text-xs text-muted-foreground">The link works once, for 15 minutes.</p>
        <Button variant="ghost" size="sm" onClick={() => send.reset()}>
          Use another address
        </Button>
      </div>
    );
  const ready = email.trim() && (!withCode || code.trim());
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) (withCode ? signIn : send).mutate();
      }}
    >
      <label className={field}>
        Work email
        <Input
          type="email"
          autoComplete={withCode ? "username" : "email"}
          spellCheck={false}
          autoFocus={primary}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@company.com"
          className="h-11 text-[15px]"
        />
      </label>
      {withCode && (
        <label className={field}>
          Sign-in code
          <Input
            type="password"
            autoComplete="current-password"
            spellCheck={false}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="XXXX-XXXX-XXXX-XXXX-XXXX"
            className="h-11 font-mono text-[15px]"
          />
          <span className="text-xs font-normal text-muted-foreground">Yours, or the invitation code an admin gave you.</span>
        </label>
      )}
      <Button
        type="submit"
        variant={primary ? "brand" : "outline"}
        size="lg"
        className="text-[15px]"
        disabled={!ready}
        loading={send.isPending || signIn.isPending}
      >
        {withCode ? "Sign in" : "Email me a sign-in link"}
      </Button>
      {byEmail && byCode && (
        <button
          type="button"
          className="w-fit self-center text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          onClick={() => {
            setWithCode(!withCode);
            send.reset();
            signIn.reset();
          }}
        >
          {withCode ? "Email me a link instead" : "Sign in with a code instead"}
        </button>
      )}
      <Outcome error={send.error ?? signIn.error} />
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
      className="flex w-full max-w-sm flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (secret.trim()) redeem.mutate();
      }}
    >
      <label className={field}>
        Admin token
        <Input type="password" autoComplete="off" spellCheck={false} autoFocus value={secret} onChange={(e) => setSecret(e.target.value)} />
      </label>
      <Button type="submit" variant="outline" size="sm" disabled={!secret.trim()} loading={redeem.isPending}>
        Sign in as admin
      </Button>
      <Outcome error={redeem.error} />
    </form>
  );
}

export function SignInPage({ me }: { me: Me }) {
  const params = new URLSearchParams(window.location.search);
  const token = params.get("t");
  const error = params.get("error");
  const methods: SignIn[] = me.sign_in_methods?.length ? me.sign_in_methods : me.sign_in ? [me.sign_in] : [];
  if (window.location.pathname !== "/signin") {
    try {
      localStorage.setItem(AFTER, window.location.pathname + window.location.search);
    } catch {
      // storage blocked: sign-in returns home
    }
  }
  if (token && methods.includes("email")) return <Confirm token={token} />;
  const provider = methods.includes("oidc");
  const byEmail = methods.includes("email");
  const byCode = methods.includes("code");
  return (
    <Shell footer={<AdminToken />}>
      <div className="flex flex-col gap-6">
        <Heading title="Sign in to comindly">Your team invites you; sign in with the account it invited.</Heading>
        {error && <Notice ok={false} message={error} />}
        {provider && <ByProvider label={me.oidc_label ?? "your company account"} brand={me.oidc_brand} />}
        {provider && (byEmail || byCode) && <Divider>or with your email address</Divider>}
        {(byEmail || byCode) && <ByAddress email={byEmail} code={byCode} primary={!provider} />}
      </div>
    </Shell>
  );
}
