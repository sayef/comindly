import { useMutation, useQuery } from "@tanstack/react-query";
import { Terminal } from "lucide-react";
import { Notice, Outcome } from "@/components/notice";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { Shell } from "@/pages/signin";

/** Approve or deny a command line's sign-in, opened from the address `comindly login` prints. */
export function DevicePage() {
  const code = new URLSearchParams(window.location.search).get("code") ?? "";
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const info = useQuery({ queryKey: ["device", code], queryFn: () => api.deviceInfo(code), enabled: !!code, retry: false });
  const answer = useMutation({ mutationFn: (approve: boolean) => api.deviceApprove(code, approve), meta: { inline: true, quiet: true } });
  return (
    <Shell>
      <div className="flex items-center gap-2">
        <Terminal className="size-4 text-muted-foreground" />
        <h1 className="text-base font-semibold">Sign in a command line</h1>
      </div>
      {answer.isSuccess ? (
        <div className="mt-4">
          <Notice ok message={answer.variables ? "Approved. You can go back to the terminal." : "Denied. The command line was not signed in."} />
        </div>
      ) : info.isError || !code ? (
        <div className="mt-4">
          <Outcome error={info.error ?? new Error("No code in this address; run `comindly login` again.")} />
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
