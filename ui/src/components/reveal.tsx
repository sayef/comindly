import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

/** A secret shown once, with a copy button. */
export function Reveal({ label, value, hint }: { label: string; value: string; hint?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex flex-col gap-1.5 rounded-md bg-muted p-3">
      <span className="text-sm font-semibold">{label}</span>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 truncate font-mono text-sm">{value}</code>
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigator.clipboard.writeText(value).then(() => setCopied(true))}
          aria-label={copied ? "Copied" : "Copy"}
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  );
}
