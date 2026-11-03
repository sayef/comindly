import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as React from "react";
import { cn } from "@/lib/utils";

export const Dialog = DialogPrimitive.Root;
export const DialogTitle = DialogPrimitive.Title;

export function DialogContent({ className, children, ...props }: React.ComponentProps<typeof DialogPrimitive.Content>) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-[rgb(6_14_12/0.55)]" />
      <DialogPrimitive.Content
        className={cn(
          "fixed left-1/2 top-[15%] z-50 w-[calc(100%-2rem)] max-w-xl -translate-x-1/2 overflow-hidden overscroll-contain rounded-xl bg-popover shadow-float",
          className,
        )}
        aria-describedby={undefined}
        {...props}
      >
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
