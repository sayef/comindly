import { useEffect, useRef } from "react";
import { useTheme } from "@/components/theme";
import { DARK, DARK_PANEL, LIGHT, LIGHT_PANEL, rippleGraph } from "@/lib/ripple-graph";

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

/** The graph, calmer, filling its positioned parent; a veil keeps text on the left readable. */
export function PanelGraph() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { theme } = useTheme();
  const palette = useRef(theme === "dark" ? DARK_PANEL : LIGHT_PANEL);
  palette.current = theme === "dark" ? DARK_PANEL : LIGHT_PANEL;
  useEffect(() => rippleGraph(canvas.current!, () => palette.current, false, true), []);
  return (
    <div className="absolute inset-0" aria-hidden="true">
      <canvas ref={canvas} className="size-full" />
      <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(100deg,var(--color-muted)_5%,color-mix(in_srgb,var(--color-muted)_60%,transparent)_40%,transparent_75%)]" />
    </div>
  );
}
