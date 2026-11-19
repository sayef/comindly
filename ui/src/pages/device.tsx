import { useMutation, useQuery } from "@tanstack/react-query";
import { Terminal } from "lucide-react";
import { useState } from "react";
import { Notice, Outcome } from "@/components/notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { Shell } from "@/pages/signin";

/** Approve or deny a command line's sign-in, opened from the address `comindly login` prints, or by typing its code. */
export function DevicePage() {
  const [code, setCode] = useState(() => new URLSearchParams(window.location.search).get("code") ?? "");
  const [typed, setTyped] = useState("");
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const info = useQuery({ queryKey: ["device", code], queryFn: () => api.deviceInfo(code), enabled: !!code, retry: false });
  const answer = useMutation({ mutationFn: (approve: boolean) => api.deviceApprove(code, approve), meta: { inline: true, quiet: true } });
  return (
    <Shell>
      <div className="flex items-center gap-2">
        <Terminal className="size-4 text-muted-foreground" />
        <h1 className="text-[17px] font-semibold">Sign in a command line</h1>
      </div>
      {answer.isSuccess ? (
        <div className="mt-4">
          <Notice ok message={answer.variables ? "Approved. You can go back to the terminal." : "Denied. The command line was not signed in."} />
        </div>
      ) : !code ? (
        <form
          className="mt-3 flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            const c = typed.trim().toUpperCase();
            if (!c) return;
            window.history.replaceState(null, "", `?code=${encodeURIComponent(c)}`);
            setCode(c);
          }}
        >
          <p className="text-sm text-muted-foreground">Enter the code the terminal shows.</p>
          <Input autoFocus value={typed} onChange={(e) => setTyped(e.target.value)} placeholder="ABCD-EFGH" aria-label="Code" className="font-mono uppercase" autoComplete="off" />
          <Button type="submit" variant="brand" disabled={!typed.trim()}>
            Continue
          </Button>
        </form>
      ) : info.isError ? (
        <div className="mt-4">
          <Outcome error={info.error} />
          <Button variant="outline" className="mt-3 w-full" onClick={() => (setCode(""), setTyped(""), window.history.replaceState(null, "", window.location.pathname))}>
            Enter another code
          </Button>
        </div>
      ) : (
        <>
          <p className="mt-2 text-sm text-muted-foreground">
            <span className="font-medium text-foreground">{info.data?.client ?? "…"}</span> asks to act as{" "}
            <span className="font-medium text-foreground">{me.data?.email}</span>. Approve only if you started it, and the terminal shows the
            code <span className="font-mono font-medium text-foreground">{code}</span>.
          </p>
          <div className="mt-5 flex gap-2">
            <Button variant="brand" className="flex-1" disabled={!info.data} loading={answer.isPending && answer.variables} onClick={() => answer.mutate(true)}>
              Approve
            </Button>
            <Button variant="outline" className="flex-1" disabled={!info.data} loading={answer.isPending && !answer.variables} onClick={() => answer.mutate(false)}>
              Deny
            </Button>
          </div>
          <div className="mt-3">
            <Outcome error={answer.error} />
          </div>
        </>
      )}
    </Shell>
  );
}
