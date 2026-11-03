---
version: 1
name: comindly app design system
description: >
  Slate: cool blue-grey neutrals in both themes, quiet chrome, and one steel-blue accent reserved for the
  primary action, the current place and contracts. Atkinson Hyperlegible Next
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

Inventory: the logo's ring of blue ellipses; repositories, each with its own colour; contracts
(HTTP routes, queues, topics, buckets, tables) as the crossings between repositories; ledger
records with kind, trust (T0 to T3) and status; `path:line @ commit` citations; symbol ids;
measured conventions ("37 of 37"); index versions; blast radius ("5 repositories, 13 references").

## Colour

```yaml
colors:            # light            dark
  ground:          ["#F4F5F6",       "#0E0F10"]   # the page; cool blue-grey
  surface:         ["#FFFFFF",       "#151618"]   # cards and tables, one tone step up, with a tinted border
  step:            ["#EAECEE",       "#1D2023"]   # hover, selected rows, quiet fills
  ink:             ["#151719",       "#E6E8EA"]   # text; blue-tinted near-black
  ink-secondary:   ["#4F5660",       "#A0A6AE"]   # supporting and meta text, >= 4.5:1 on ground and step
  accent:          ["#2F5F9E",       "#8DB4EA"]   # steel blue: primary action, current place, contracts
  on-accent:       ["#FFFFFF",       "#0E0F10"]
  border:          ["#DDDFE3",       "#26282C"]   # card outlines, header bars, row dividers
  rail:            ["#EDEFF1",       "#0B0C0D"]   # the side rail, one step off the ground
  control-edge:    ["#C9CDD2",       "#373B41"]   # field and select outlines, one step stronger than border
  popover:         ["#FFFFFF",       "#1C1E20"]   # menus, dialogs and the search palette
  warning:         ["#8F5400",       "#E5AE4A"]
  error:           ["#B42318",       "#F27A7C"]
  focus:           accent
  # In code (ui/src/index.css) these are --background, --card, --muted, --foreground, --muted-foreground,
  # --brand, --brand-foreground, --border, --sidebar, --input and --popover.
```

Chosen from six palettes after a pine green read as too green. The logo ring uses the same blues.

Repositories get one of eight muted tones (sage, plum, olive, sand, stone, moss, clay, teal),
always the same one, shown only as a small square beside the name in lists. Never as fills, never a
rainbow. Graphs stay neutral: grey for code, the accent for contracts, amber for cycles.

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
    small:     { size: 13px, weight: 400, lineHeight: 1.4 }                        # meta, table detail, badges
    label:     { size: 12px, weight: 500, lineHeight: 1.4 }                        # field labels only
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
- Navigation: a bordered rail with icons; the current page on the accent tint. It collapses to an icon
  rail (remembered per browser); on phones it opens as a drawer from a menu button, never a sideways strip.
- Phones: tables become two-line rows (`<Table reflow>`, cells placed with `Td slot`); filters become
  selects or a two-column grid; nothing scrolls sideways.
- Irreversible actions confirm in place (`ConfirmButton`), not in a modal.
- Badges mark exceptions (stale, disputed, unconfirmed), never the normal state.
- Org map: a focus graph, one repository at a time (users above, what it uses below, staggered rows whose
  arrows pass through the gaps), with the repository list beside it; even thin arrows, contracts on hover.
  A whole-org drawing tangles on real indexes (one library used by 14, cycles), so it is not offered.
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
