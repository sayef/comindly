---
version: 1
name: comindly app design system
description: >
  Pine ink on a cool paper ground in light, the same pine as the ground in dark. One accent, the
  logo's green, reserved for the primary action and the current place. Atkinson Hyperlegible Next
  for text and Atkinson Hyperlegible Mono for code, because the app is read in identifiers, paths
  and hashes where 1, l, I, 0 and O must never be confused. The layout is the app's established one:
  a bordered rail, a centred search, bordered cards with a header bar.
mode: operate
taste: { bold: 3, motion: 2, density: 7 }
---

# comindly app: brand tokens

## Subject

Engineers and their agents asking a cross-repository index what depends on what, and the ledger
why code is the way it is. Read at a desk, in long sessions, beside an editor, in either theme.

Inventory: the logo's ring of green ellipses; repositories, each with its own colour; contracts
(HTTP routes, queues, topics, buckets, tables) as the crossings between repositories; ledger
records with kind, trust (T0 to T3) and status; `path:line @ commit` citations; symbol ids;
measured conventions ("37 of 37"); index versions; blast radius ("5 repositories, 13 references").

## Colour

```yaml
colors:            # light            dark
  ground:          ["#F4F6F4",       "#0B1311"]   # the page; cool paper / pine night
  surface:         ["#FFFFFF",       "#121C19"]   # cards and tables, one tone step up, with a tinted border
  step:            ["#EAEEEB",       "#1A2623"]   # hover, selected rows, quiet fills
  ink:             ["#0F1F1A",       "#E4ECE8"]   # text; deep pine, not neutral black
  ink-secondary:   ["#4A5B55",       "#A7B6B0"]   # supporting text, tinted from ink
  ink-muted:       ["#5B6B65",       "#8A9A94"]   # meta and placeholders, still >= 4.5:1
  accent:          ["#0F6B50",       "#5CCFA4"]   # the logo's green: primary action, current place
  on-accent:       ["#FFFFFF",       "#04201A"]
  border:          ["#DDE3DF",       "#22302C"]   # card outlines, header bars, inputs, row dividers
  warning:         ["#8F5400",       "#E5AE4A"]
  error:           ["#B42318",       "#F27A7C"]
  info:            ["#2F5FA8",       "#8FB3F5"]
  focus:           accent
```

Repositories get one of eight muted tones (sage, slate blue, olive, sand, stone, moss, clay, teal),
always the same one, shown only as a small square beside the name in lists. Never as fills, never a
rainbow. Graphs stay neutral: grey for code, green for contracts, amber for cycles.

## Typography

```yaml
typography:
  text: { family: "Atkinson Hyperlegible Next Variable", fallback: "ui-sans-serif, system-ui, sans-serif" }
  code: { family: "Atkinson Hyperlegible Mono Variable", fallback: "ui-monospace, SFMono-Regular, Menlo, monospace" }
  scale:
    statement: { size: 34px, weight: 500, lineHeight: 1.15, tracking: "-0.02em" }  # the page's finding
    title:     { size: 26px, weight: 600, lineHeight: 1.2,  tracking: "-0.02em" }  # page title
    heading:   { size: 17px, weight: 600, lineHeight: 1.35 }                       # sections
    body:      { size: 14px, weight: 400, lineHeight: 1.5 }                        # rows, prose
    small:     { size: 12px, weight: 500, lineHeight: 1.4 }                        # labels, meta, badges
  numerals: tabular-nums wherever figures are compared
  rules:
    - mono only for code: symbols, paths, hashes, routes, commands
    - labels in sentence case, never tracked capitals
```

## Shape and depth

```yaml
radius: { surface: 10px, control: 7px, chip: 5px }
elevation:
  rest: "surface one tone step above ground, 1px tinted border, no shadow"
  floating: "0 1px 2px rgb(15 31 26 / .08), 0 12px 32px rgb(15 31 26 / .14)"  # menus, dialogs, palette
spacing: { base: 4px, component: [4, 8, 12, 16, 24], section: [32, 48] }
```

## Components

- Buttons: flat fill; primary in accent, the rest a quiet step fill or ghost; spinner beside the label.
- Tables sit on a surface; rows divide with a tinted 1px line, hover with the step tone.
- Badges are words in a chip; colour only with a word, never alone.
- Navigation: a bordered rail with icons; the current page on the accent tint.
- Org map: a layered left-to-right graph (dagre) of neutral boxes and even, thin arrows; detail on hover
  (contracts behind an arrow) and on click (what a repository uses and what uses it).
- Icons: lucide, one stroke weight. No arrows glued to link text, no middle-dot meta strings.

## Motion

State changes only: 120 ms for hover and press, 200 ms for opening. `prefers-reduced-motion`
keeps the change and drops the travel.

## Browser surfaces

Selection in accent at 25 %, caret in accent, focus ring 2 px accent with 2 px offset,
`theme-color` and `color-scheme` follow the theme.

## Principles

1. Keep the established layout; the theme carries the change.
2. Code looks like code and nothing else does.
3. A repository has one colour everywhere.
4. Page headers are a title and one plain sentence; findings sit with the thing they describe.
