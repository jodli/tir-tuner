---
name: The artificial pancreas project, for patients
description: Type 1 diabetes explained in the furniture of a clinic glucose report.
colors:
  ink: "#1d1d1b"
  muted: "#55534f"
  hair: "#c9c7c2"
  paper: "#ffffff"
  band: "#e9f4e7"
  spread-outer: "#e4e3df"
  spread-inner: "#c3c1bb"
  very-low: "#8b1a1a"
  low: "#e53935"
  in-range: "#4caf50"
  high: "#f9d648"
  very-high: "#f28c28"
  high-limit: "#c9a400"
typography:
  display:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "clamp(1.9rem, 6vw, 2.6rem)"
    fontWeight: 800
    lineHeight: 1.08
    letterSpacing: "-0.015em"
    fontFeature: "tnum"
  headline:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "1.3rem"
    fontWeight: 700
    lineHeight: 1.25
    fontFeature: "tnum"
  body:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.6
    fontFeature: "tnum"
  caption:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.45
    fontFeature: "tnum"
  label-strip:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 600
    letterSpacing: "0.1em"
    fontFeature: "tnum"
  label-panel:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 700
    letterSpacing: "0.06em"
    fontFeature: "tnum"
  label-nav:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 600
    fontFeature: "tnum"
  label-group:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 700
    fontFeature: "tnum"
rounded:
  none: "0"
spacing:
  gap: "clamp(1rem, 4vw, 1.5rem)"
  measure: "40rem"
  panel-x: "0.8rem"
  section: "2.6rem"
  figure: "1.75rem"
components:
  masthead-strip:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.label-strip}"
    rounded: "{rounded.none}"
    padding: "0.7rem clamp(1rem, 4vw, 1.5rem)"
  range-bar:
    height: "0.7rem"
    rounded: "{rounded.none}"
  chapter-tab:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    typography: "{typography.label-nav}"
    padding: "0.55rem 0"
  chapter-tab-hover:
    backgroundColor: "{colors.band}"
  chapter-tab-current:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
  report-panel:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
  report-panel-title:
    typography: "{typography.label-panel}"
    padding: "0.65rem 0.8rem 0.6rem"
  turn-link:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    padding: "0.8rem"
  turn-link-hover:
    backgroundColor: "{colors.band}"
---

# Design System: The artificial pancreas project, for patients

## Overview

**Creative North Star: "The Clinic Report"**

Every page is set like the one-page glucose report a diabetes clinic prints (AGP style). The report's own furniture carries the explanation: a near-black header strip, the stacked time in range bar in the five report colours, ruled panels with the 70 to 180 band tinted pale green, a heavy ink median and a grey spread of days. A patient recognises it; a friend learns to read it on page one and reuses that skill on every chapter.

The world is flat, white and ruled. Ink does the structural work, hairlines separate, and colour is reserved for glucose meaning. One single-column reading measure, one sans family with tabular figures, square corners everywhere. Density is a printout's: compact labels and tables next to generous body text.

Every figure is virtual patient model output and says so in its own footer line.

**Key Characteristics:**
- White paper ground, ink rules, no shadows, no radius.
- The five range colours mean glucose ranges and nothing else.
- Colour never alone: every band has a text label, lows also carry a stripe pattern.
- One glucose scale (mg/dL primary, mmol/L in small type under each tick) on every glucose figure.
- Every figure has its numbers in a table under it.

## Colors

Ink on white with a strict, data-only clinical range palette.

### Primary
- **Report Ink** (ink): text, 1px panel rules, the header strip, the current chapter tab, median curves, focus outlines.

### Secondary
- **Range Colours** (very-low, low, in-range, high, very-high): the clinic report's five glucose bands. Used only in the range bar, the per-figure TIR bars and the limit lines. Very low and low always carry the white diagonal stripe (`repeating-linear-gradient(135deg, transparent 0 3px, rgb(255 255 255 / 0.55) 3px 4.5px)`).
- **Darkened High Rule** (high-limit): the 180 mg/dL limit line on charts. The high yellow is invisible as a 2px line on white, so the line uses this darker gold. The 70 line uses low red.

### Neutral
- **Paper** (paper): page ground, panel ground, text on the ink strip.
- **Report Grey** (muted): figure captions, axis ticks, secondary lines (the muted "before" curve), the standing note, the "show the numbers" summary, the playground's hint, empty-state, unit and disabled-chip text, the resting delete icon.
- **Hairline** (hair): table row rules, chapter tab dividers, grid lines, inner panel dividers, scrollbar thumb, the chip borders, the playground's group and event-row dividers and input borders.
- **Target Band Green** (band): the 70 to 180 band behind glucose curves, and the hover fill for chapter tabs, playground chips, mode toggle cells, the delete button and prev/next links.
- **Spread Greys** (spread-outer, spread-inner): the 5 to 95 and 25 to 75 percentile spread of days behind the median.

### Named Rules
**The Range Colours Are Data Rule.** The five range colours appear only where they encode a glucose range. Never as accent, link, button or decoration. Form controls follow: the demo slider's accent is ink, precisely because range colours are data-only.

**The Never Colour Alone Rule.** Every coloured band carries a text label or a legend entry; lows also carry the stripe pattern.

**The Ink Accent Rule.** There is no brand accent. Emphasis is ink: weight, the inverted ink tab, the heavy median.

## Typography

**Display Font:** Public Sans (self-hosted variable woff2, with system-ui, sans-serif)
**Body Font:** Public Sans
**Label/Mono Font:** Public Sans, tracked caps for report labels

**Character:** One plain, government-report sans at every level, with tabular numerals site-wide so numbers line up like a printout.

### Hierarchy
- **Display** (800, clamp(1.9rem, 6vw, 2.6rem), 1.08, balanced wrap): page title, once per page.
- **Headline** (700, 1.3rem, 1.25): section heads inside the reading column.
- **Body** (400, 1.0625rem, 1.6; 1.125rem from 60rem wide): running text in a 40rem measure.
- **Caption** (400, 0.875rem, 1.45, report grey): panel subtitles, the standing note.
- **Strip label** (600, 0.75rem, 0.1em, uppercase): header strip, chapter "n/10" and page name, language switch.
- **Panel label** (700, 0.8125rem, 0.06em, uppercase): report panel titles, table column heads (0.75rem), the "virtual patient, model output" footer (400, 0.75rem, 0.04em).
- **Group label** (700, 0.9375rem, no tracking, sentence case): the playground's fieldset legends. Sits between Panel label and Headline; neither folds it in, since Headline is 1.3rem and the 0.9375rem table size is 400.
- **Chart text** (11px ticks, 9px sub-ticks, 12px 600 labels): inside SVG figures.

### Named Rules
**The Report Label Rule.** Tracked uppercase belongs to report furniture only: the strip, panel titles, table heads, the operate control's label, the mode toggle and the provenance line. Body headings stay sentence case, and so do control-group legends inside a panel ("Your day", "Pump"): they name a group the reader works in, not a column of the report.

**The Tabular Numbers Rule.** `font-variant-numeric: tabular-nums` is set on body and inherited everywhere.

## Layout

Single reading column: max width 40rem plus two gutters, centred, gutter `clamp(1rem, 4vw, 1.5rem)`. The masthead runs full bleed; the chapter strip's tabs sit inside the column width. Main starts 1.75rem below the masthead; h2 sections open with 2.6rem above. Figures sit at full column width with 1.75rem above and 2rem below. Panel internals use a 0.8rem side inset.

Phone first (390px reference). Charts take a 0.72 aspect under 520px host width and 0.48 above. At 60rem the body size steps up to 1.125rem; nothing else reflows. Wide tables inside panels scroll horizontally in their own box.

## Elevation & Depth

Flat. No box shadows anywhere. Depth comes from ink: 1px ink rules around panels, 1px hairlines inside them, and inversion (ink ground, paper text) for the header strip and current chapter.

**The Printout Rule.** If it would not survive a black and white laser printer, it is not structure. Shadows, gradients and glows have no place; the only gradient is the low-range stripe pattern.

## Shapes

Square corners throughout (radius 0). Containers are rectangles drawn with 1px ink rules. The chapter you stopped at carries a small ink corner triangle (0.4rem) at the tab's top right. Arrows on prev/next are inline SVG strokes (1.8 width, round caps). Bolus markers on the playground chart are small ink triangles pointing down onto the axis. The playground's delete button is a drawn SVG cross (12 unit box, 1.8 stroke, round caps, the arrows' stroke), never a typed glyph.

## Components

### Masthead
- **Header strip:** ink ground, paper text, strip label type, "n/10 · page name" left (truncates with ellipsis), language switch right. Current language underlined 2px in paper; the other at 75% opacity, full on hover.
- **Range bar:** 0.7rem tall, five flex segments sized from the virtual patient's bins, very low and low striped. Decorative (aria-hidden); the range key under it says the numbers in text.
- **Range key:** 0.8125rem centred line, in-range share in bold, hairline under it.

### Navigation
- **Chapter strip:** ten equal numbered tabs, 600 0.8125rem, hairline dividers, ink rule under the strip. Hover fills band green; the current chapter is an inverted ink tab; the stopped-at chapter carries the corner triangle. Chapter names are screen-reader text and title tooltips.
- **Prev/next (turn):** two-cell grid with a 1px ink frame and ink gutter, 700 links with SVG arrows, next aligned right. Hover fills band green.

### Report Panel (signature)
- **Frame:** 1px ink rule, square, white.
- **Title:** panel label in tracked caps, caption subtitle in grey under it, ink rule below.
- **Chart:** band green 70 to 180 rect, darkened high rule at 180 and low red at 70 (2px), hairline grid, ink axis, mg/dL ticks with mmol/L sub-ticks, spread greys behind, lines in ink (heavy median; others dashed, dotted or grey), meal ticks with grams, open dots in paper with ink stroke.
- **Legend:** inline line samples (28x10 SVG) with names, wraps.
- **TIR bars:** per-case range bars (0.9rem) with labels.
- **Show the numbers:** a details element, grey 600 summary, hairline above, holding the data table.
- **Provenance line:** hairline above, grey 0.75rem tracked caps "virtual patient · model output".

### Operate Panel (demo chapter)
A report panel that re-runs its day on input. Same frame, title, chart, legend and provenance line as the report panel; the differences:
- **Control row:** tracked-caps label, native range input with `accent-color: var(--ink)`, bold output with `min-width: 7ch`; 0.65rem 0.8rem 0.55rem inset, hairline below.
- **Two-curve pairing:** the unchanged day is the muted 1.4px base ("as set"), the changed day the heavy ink 2.8px curve ("with your change"); legend samples carry the same weights.
- **Comparison table:** always visible (no details disclosure), two value columns "as set" / "with your change", table styles as below.
- **Recalculating:** while a run is pending the panel carries `calculating` and the plot dims to opacity 0.5 (150 ms ease-out).
- **Failure:** the slider disables and a grey error line with `role="status"` takes the figure's place.

### Playground Panel (play chapter)
A report panel the reader composes: ready-made days, an event editor of meals and boluses, the pump mode and the calculator and resistance sliders, and the chart answers with the same two-curve vocabulary as the operate panel. Same frame, title, chart furniture, provenance line and `calculating` dim as the operate panel; the additions:
- **Groups:** controls sit in borderless fieldsets divided by hairlines, each opened by a Group label legend in sentence case: "Start from a ready-made day", "Your day", "Pump", "Bolus calculator", "What if" (German equivalents).
- **Sticky view:** legend, chart and summary line sit in one paper-ground view straight under the presets, ink rule below. From 40rem viewport height it is `position: sticky` at the top, so the chart stays in sight while the groups below change the day; shorter screens let it scroll.
- **Summary line:** the masthead's range key format ("**n%** in range · n% below · n% above"), live as `role="status"`.
- **Before and now:** the day before the last change is the muted 1.4px curve under the 2.8px ink "Now" curve, with legend samples at the same weights; one slider drag counts as one change. The play table gains "Before" / "Now" columns; with no change yet it has a single "Now" column. Rows: in range, below 70, above 180, bolus insulin, mean glucose.
- **Chips:** square buttons, 600 0.8125rem, hairline border, no fill, 2.25rem tall; hover fills band green. Preset chips carry `aria-pressed` and the pressed one inverts to ink, like the mode toggle; any edit clears it. "Add meal" / "Add bolus" close the editor group and place the new row 3 h after the last event (07:00 when empty).
- **Mode toggle:** a two-cell segmented control in a 1px ink frame ("Auto mode" / "Basal only"), tracked caps 600 0.8125rem, ink divider, 2.25rem tall; hover fills band green, the active cell is an inverted ink tab (`aria-pressed`). A grey caption-size hint beside it (`aria-describedby`) says what the current mode does.
- **Event rows:** a grid of bold kind label (Meal / Bolus), time input (15 min steps), amount input with grey unit, the suggestion chip and the delete button, hairline between rows, 0.875rem. Inputs are hairline-bordered, unfilled, 2.25rem tall. Below 34rem the chip drops to a second row under time and amount. An empty editor shows a grey line pointing to the add buttons and presets.
- **Suggestion chip:** meal rows only, named for the action: "Give 5.5 U", or "Set to 6.8 U" when a bolus already sits at that minute. When there is nothing to do it disables and renders frameless in report grey: "Given", "No bolus suggested", "Too low for a bolus" (the calculator refuses below its minimum glucose). Chips refresh in place after each run; rebuilt rows return focus to the same event and field.
- **Delete:** a 2.25rem square button holding the drawn SVG cross, grey at rest, ink on band green on hover, labelled "Delete the entry at 07:00".
- **Bolus markers:** small ink triangles pointing down onto the axis with grey dose labels, centred. When a meal and a bolus share a minute, the gram label stacks above the dose label, both centred on the tick.
- **Failure:** every control disables and a grey `role="status"` line explains; a failed engine load and a failed run have separate messages.

### Tables
Full width, 0.9375rem, hairline row rules, ink rule under uppercase 0.75rem heads, left aligned, no zebra, no fills.

### Diagram panel
Mermaid flow diagrams sit in the same 1px ink frame, themed to white nodes, ink borders and lines, band green secondary. Kept only where a flow says it better than a curve.

### Links and focus
Links inherit ink, 1px underline at 0.18em offset, 2px on hover. Focus is a 2px ink outline at 2px offset (paper on the strip). Selection is high yellow with ink text.

## Do's and Don'ts

### Do:
- **Do** draw every glucose figure on the one glucose scale with the band green 70 to 180 band and the two limit rules.
- **Do** give every figure a numbers table and the virtual patient provenance line.
- **Do** label every coloured band in text, and stripe the lows.
- **Do** build containers from 1px ink rules on white with square corners.
- **Do** keep emphasis in ink: weight, inversion, line thickness.

### Don't:
- **Don't** use the five range colours for anything but glucose ranges.
- **Don't** add shadows, rounded cards, or tinted card grounds.
- **Don't** add a brand accent colour or a blue link colour.
- **Don't** add decorative icons; the only icons are functional drawn SVGs: the prev/next arrows and the playground's delete cross.
- **Don't** use tracked uppercase outside report furniture (strip, panel title, table head, operate control label, mode toggle, provenance line).
- **Don't** use the high yellow (#f9d648) as a line or text colour on white; use the darkened high rule.
