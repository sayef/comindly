import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useState } from "react";
import { cn, sentence } from "@/lib/utils";

/** Wraps long unbroken text (URLs, paths, ids) inside its box. */
export const wrap = "min-w-0 [overflow-wrap:anywhere]";

/** A success or warning line; long messages fold to three lines until clicked. */
export function Notice({ ok, message, className }: { ok: boolean; message: string; className?: string }) {
  const [open, setOpen] = useState(false);
  const long = message.length > 240;
  return (
    <div role={ok ? "status" : "alert"} className={cn("flex items-start gap-1.5 text-xs", ok ? "text-muted-foreground" : "text-warning", className)}>
      {ok ? <CheckCircle2 className="mt-px size-3.5 shrink-0 text-brand" /> : <AlertTriangle className="mt-px size-3.5 shrink-0" />}
      <div className={cn(wrap, "flex-1")}>
        <p className={cn(long && !open && "line-clamp-3")}>{sentence(message)}</p>
        {long && (
          <button type="button" className="mt-0.5 underline underline-offset-2 opacity-80 hover:opacity-100" onClick={() => setOpen(!open)}>
            {open ? "Less" : "Show all"}
          </button>
        )}
      </div>
    </div>
  );
}

/** The outcome of a mutation: its error, or its success message. */
/** An action's error, shown where the action is; successes appear as toasts. */
export function Outcome({ error }: { error?: Error | null; message?: string }) {
  if (!error || error.name === "AbortError") return null;
  return <Notice ok={false} message={error.message} />;
}
