import { useQuery } from "@tanstack/react-query";
import { KeyRound, X } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { api, type TokenStatus } from "@/lib/api";
import { useAccess } from "@/lib/me";

const HIDDEN = "comindly-token-notice-hidden";

/** What is wrong with a token, or nothing when it is fine for more than two weeks. */
function problem(t: TokenStatus): string | null {
  if (t.expired) return `The ${t.host} token has expired, so repositories there are no longer fetched.`;
  if (t.refused) return `${t.host} refuses the stored token (revoked, or without access to the repositories).`;
  if (t.days_left != null && t.days_left <= 14)
    return `The ${t.host} token expires in ${t.days_left === 1 ? "1 day" : `${t.days_left} days`}.`;
  return null;
}

/** For admins on a server: a git host token that has expired, is refused, or runs out soon. */
export function TokenNotice() {
  const { server, admin } = useAccess();
  const status = useQuery({ queryKey: ["token-status"], queryFn: api.tokenStatus, enabled: server && admin, staleTime: 60 * 60_000, retry: false });
  const [hidden, setHidden] = useState(() => {
    try {
      return sessionStorage.getItem(HIDDEN) === "1";
    } catch {
      return false;
    }
  });
  const bad = (status.data ?? []).map((t) => [t, problem(t)] as const).find(([, p]) => p);
  if (!bad || hidden) return null;
  const [t, text] = bad;
  const hide = () => {
    try {
      sessionStorage.setItem(HIDDEN, "1");
    } catch {
      // storage may be unavailable
    }
    setHidden(true);
  };
  return (
    <div role="status" className="mb-5 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-warning/40 bg-warning/10 px-4 py-2.5 text-sm">
      <KeyRound className="size-4 shrink-0 text-warning" />
      <span className="min-w-0 flex-1">{text}</span>
      <Button asChild size="sm" variant="outline">
        <Link to={`/settings/integrations#secret-${t.name}`}>Update token</Link>
      </Button>
      <Button size="icon" variant="ghost" className="size-8" onClick={hide} aria-label="Hide until the next visit">
        <X />
      </Button>
    </div>
  );
}
