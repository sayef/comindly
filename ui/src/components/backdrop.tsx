import { useEffect, useRef } from "react";
import { useTheme } from "@/components/theme";
import { DARK, LIGHT, rippleGraph } from "@/lib/ripple-graph";

/** The repository graph behind sign-in and empty pages, with a veil where the content sits. */
export function Backdrop() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { theme } = useTheme();
  const palette = useRef(theme === "dark" ? DARK : LIGHT);
  palette.current = theme === "dark" ? DARK : LIGHT;
  useEffect(() => rippleGraph(canvas.current!, () => palette.current), []);
  return (
    <div className="absolute inset-0" aria-hidden="true">
      <canvas ref={canvas} className="size-full" />
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_40%_50%_at_50%_50%,var(--color-background)_45%,transparent_100%)]" />
    </div>
  );
}
