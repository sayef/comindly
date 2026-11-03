import * as React from "react";
import { cn } from "@/lib/utils";

const Reflow = React.createContext(false);

/** With `reflow`, rows become two-line list rows on phones; place cells with `Td slot`. */
export function Table({ className, reflow = false, ...props }: React.HTMLAttributes<HTMLTableElement> & { reflow?: boolean }) {
  return (
    <Reflow.Provider value={reflow}>
      <div className={cn("w-full", reflow ? "sm:overflow-x-auto" : "overflow-x-auto")}>
        <table className={cn("w-full text-sm", reflow && "max-sm:block max-sm:[&>tbody]:block max-sm:[&>thead]:hidden", className)} {...props} />
      </div>
    </Reflow.Provider>
  );
}
export function Th({ className, ...props }: React.ThHTMLAttributes<HTMLTableCellElement>) {
  return <th className={cn("eyebrow h-10 px-3 text-left align-middle sm:first:pl-5 sm:last:pr-5", className)} {...props} />;
}

/** Where a cell sits on a phone: line one is lead, main and end; meta cells share line two, in order-4 and up. */
export type Slot = "lead" | "main" | "end" | "meta" | "full" | "desktop";
const SLOT: Record<Slot, string> = {
  lead: "max-sm:order-0 max-sm:shrink-0",
  main: "max-sm:order-1 max-sm:min-w-0 max-sm:flex-1",
  end: "max-sm:order-2 max-sm:shrink-0",
  meta: "max-sm:order-4 max-sm:mt-1 max-sm:min-w-0 max-sm:max-w-full max-sm:text-xs max-sm:text-muted-foreground",
  full: "max-sm:order-9 max-sm:mt-2 max-sm:basis-full",
  desktop: "max-sm:hidden",
};

export function Td({ className, slot, ...props }: Omit<React.TdHTMLAttributes<HTMLTableCellElement>, "slot"> & { slot?: Slot }) {
  const reflow = React.useContext(Reflow);
  return <td className={cn("px-3 py-2.5 align-top sm:first:pl-5 sm:last:pr-5", reflow && slot && cn("max-sm:block max-sm:p-0", SLOT[slot]), className)} {...props} />;
}
export function Tr({ className, ...props }: React.HTMLAttributes<HTMLTableRowElement>) {
  const reflow = React.useContext(Reflow);
  return (
    <tr
      className={cn(
        "border-b last:border-0 transition-colors duration-100 hover:bg-muted/60",
        reflow &&
          "max-sm:flex max-sm:flex-wrap max-sm:items-center max-sm:gap-x-3 max-sm:px-4 max-sm:py-3 max-sm:after:order-3 max-sm:after:basis-full max-sm:after:content-['']",
        className,
      )}
      {...props}
    />
  );
}
