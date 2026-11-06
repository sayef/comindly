import { useEffect, useRef, useState } from "react";
import { Button, type ButtonProps } from "@/components/ui/button";

/** A button that asks once more, in place, before it runs something that cannot be undone or costs. */
export function ConfirmButton({
  question,
  confirmLabel,
  onConfirm,
  danger = true,
  children,
  ...props
}: Omit<ButtonProps, "onClick"> & { question: string; confirmLabel: string; onConfirm: () => void; danger?: boolean }) {
  const [asking, setAsking] = useState(false);
  const yes = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (asking) yes.current?.focus();
  }, [asking]);
  if (!asking)
    return (
      <Button {...props} onClick={() => setAsking(true)}>
        {children}
      </Button>
    );
  return (
    <span role="group" aria-label={question} className="inline-flex flex-wrap items-center gap-2" onKeyDown={(e) => e.key === "Escape" && setAsking(false)}>
      <span className="text-xs text-muted-foreground">{question}</span>
      <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
        Cancel
      </Button>
      <Button
        ref={yes}
        size="sm"
        variant={danger ? undefined : "brand"}
        className={danger ? "bg-danger text-white hover:bg-[color-mix(in_srgb,var(--color-danger)_85%,black)] dark:text-background" : undefined}
        loading={props.loading}
        onClick={() => (setAsking(false), onConfirm())}
      >
        {confirmLabel}
      </Button>
    </span>
  );
}
