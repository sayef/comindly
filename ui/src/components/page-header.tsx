import type { ReactNode } from "react";

/** A page's title, the one sentence that says what the page found or holds, and its actions. */
export function PageHeader({ title, children, actions }: { title: ReactNode; children?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
      <div className="flex min-w-0 max-w-[80ch] flex-col gap-1.5">
        <h1 className="text-[26px] leading-tight font-semibold tracking-[-0.02em] [overflow-wrap:anywhere]">{title}</h1>
        {children && <p className="text-sm text-pretty text-muted-foreground">{children}</p>}
      </div>
      {/* At the description's level, clear of the indexing tabs that hang from the app header. */}
      {actions && <div className="flex shrink-0 flex-wrap gap-2 self-end max-sm:w-full">{actions}</div>}
    </header>
  );
}
