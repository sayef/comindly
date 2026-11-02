import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "flex h-9 w-full rounded-md border border-input bg-card px-3 text-sm placeholder:text-muted-foreground transition-[border-color,box-shadow] duration-100 hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/20 max-sm:text-base",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
