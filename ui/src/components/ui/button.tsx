import { Slot } from "@radix-ui/react-slot";
import { Loader2 } from "lucide-react";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-semibold transition-[background-color,color,box-shadow] duration-100 disabled:pointer-events-none disabled:opacity-45 [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/85",
        brand: "bg-brand text-brand-foreground hover:bg-[color-mix(in_srgb,var(--color-brand)_86%,var(--color-foreground))]",
        outline: "bg-secondary text-foreground hover:bg-[color-mix(in_srgb,var(--color-secondary)_80%,var(--color-foreground))]",
        ghost: "text-muted-foreground hover:bg-muted hover:text-foreground",
        secondary: "bg-secondary text-secondary-foreground hover:bg-[color-mix(in_srgb,var(--color-secondary)_80%,var(--color-foreground))]",
      },
      size: { default: "h-9 px-3.5", sm: "h-8 px-3 text-xs", lg: "h-11 px-5", icon: "size-9" },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
  /** Something this button started is running: show a spinner and refuse further clicks. */
  loading?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, loading = false, disabled, children, ...props }, ref) => {
    if (asChild)
      return (
        <Slot className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props}>
          {children}
        </Slot>
      );
    return (
      <button className={cn(buttonVariants({ variant, size, className }))} ref={ref} disabled={disabled || loading} aria-busy={loading} {...props}>
        {loading && <Loader2 className="animate-spin" />}
        {children}
      </button>
    );
  },
);
Button.displayName = "Button";
