/** Text with `code` spans shown as code. */
export function withCode(text: string) {
  return text.split(/`([^`]+)`/).map((part, i) =>
    i % 2 ? (
      <code key={i} className="rounded-sm bg-muted px-1 py-px font-mono text-[0.9em]">
        {part}
      </code>
    ) : (
      part
    ),
  );
}
