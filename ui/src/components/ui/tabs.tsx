import * as React from "react";
import { cn } from "@/lib/utils";

const Ctx = React.createContext<{ value: string; onValueChange: (v: string) => void } | null>(null);

/** A segmented filter: one pressed button at a time, announced as a labelled group. */
export function ToggleGroup({
  label,
  value,
  onValueChange,
  className,
  children,
}: {
  label: string;
  value: string;
  onValueChange: (v: string) => void;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <Ctx.Provider value={{ value, onValueChange }}>
      <div
        role="group"
        aria-label={label}
        className={cn(
          "inline-flex h-9 max-w-full flex-wrap pointer-coarse:h-auto items-center gap-0.5 rounded-lg bg-secondary p-0.5 text-muted-foreground max-sm:h-auto",
          className,
        )}
      >
        {children}
      </div>
    </Ctx.Provider>
  );
}

export function Toggle({ value, className, ...props }: Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "value"> & { value: string }) {
  const group = React.useContext(Ctx);
  const on = group?.value === value;
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={() => group?.onValueChange(value)}
      className={cn(
        "inline-flex h-8 items-center gap-1.5 rounded-md px-3 pointer-coarse:h-11 text-xs font-semibold whitespace-nowrap transition-colors duration-100 hover:text-foreground max-sm:px-2",
        on && "bg-card text-foreground shadow-[0_1px_2px_var(--shadow-ink)]",
        className,
      )}
      {...props}
    />
  );
}
