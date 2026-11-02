import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Stable hue per repository, so a repo keeps its colour on every graph. */
/** Muted tones that sit with the pine theme; a repository always gets the same one. */
const REPO_TONES = ["#5f9e8a", "#6b8fb0", "#9a8f5e", "#b08d6a", "#7f8f9e", "#6fa36b", "#a07a6a", "#5d8f9a"];

export function repoColor(repo: string): string {
  return REPO_TONES[repoHue(repo) % REPO_TONES.length];
}

export function repoHue(repo: string): number {
  let h = 0;
  for (const c of repo) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

export const CONTRACTS = "_contracts";

/** A message from the server as a sentence: first letter capitalized, unless it starts with code, a path or the name. */
export function sentence(text: string): string {
  if (!/^[a-z]/.test(text) || /^comindly\b/.test(text)) return text;
  return text[0].toUpperCase() + text.slice(1);
}

/** How long ago `iso` was, briefly: "just now", "4 min ago", "3 h ago", "2 d ago". */
export function ago(iso: string | null | undefined): string {
  if (!iso) return "";
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}
