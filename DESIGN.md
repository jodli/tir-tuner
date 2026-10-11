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
- **Report Grey** (muted): figure captions, axis ticks, secondary lines, the standing note, the "show the numbers" summary.
- **Hairline** (hair): table row rules, chapter tab dividers, grid lines, inner panel dividers, scrollbar thumb.
- **Target Band Green** (band): the 70 to 180 band behind glucose curves, and the hover fill for chapter tabs and prev/next links.
- **Spread Greys** (spread-outer, spread-inner): the 5 to 95 and 25 to 75 percentile spread of days behind the median.

### Named Rules
**The Range Colours Are Data Rule.** The five range colours appear only where they encode a glucose range. Never as accent, link, button or decoration.

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
- **Strip label** (600, 0.75rem, 0.1em, uppercase): header strip, chapter "n/8" and page name, language switch.
- **Panel label** (700, 0.8125rem, 0.06em, uppercase): report panel titles, table column heads (0.75rem), the "virtual patient, model output" footer (400, 0.75rem, 0.04em).
- **Chart text** (11px ticks, 9px sub-ticks, 12px 600 labels): inside SVG figures.

### Named Rules
**The Report Label Rule.** Tracked uppercase belongs to report furniture only: the strip, panel titles, table heads and the provenance line. Body headings stay sentence case.

**The Tabular Numbers Rule.** `font-variant-numeric: tabular-nums` is set on body and inherited everywhere.

## Layout

Single reading column: max width 40rem plus two gutters, centred, gutter `clamp(1rem, 4vw, 1.5rem)`. The masthead runs full bleed; the chapter strip's tabs sit inside the column width. Main starts 1.75rem below the masthead; h2 sections open with 2.6rem above. Figures sit at full column width with 1.75rem above and 2rem below. Panel internals use a 0.8rem side inset.

Phone first (390px reference). Charts take a 0.72 aspect under 520px host width and 0.48 above. At 60rem the body size steps up to 1.125rem; nothing else reflows. Wide tables inside panels scroll horizontally in their own box.

## Elevation & Depth

Flat. No box shadows anywhere. Depth comes from ink: 1px ink rules around panels, 1px hairlines inside them, and inversion (ink ground, paper text) for the header strip and current chapter.

**The Printout Rule.** If it would not survive a black and white laser printer, it is not structure. Shadows, gradients and glows have no place; the only gradient is the low-range stripe pattern.

## Shapes

Square corners throughout (radius 0). Containers are rectangles drawn with 1px ink rules. The chapter you stopped at carries a small ink corner triangle (0.4rem) at the tab's top right. Arrows on prev/next are inline SVG strokes (1.8 width, round caps).

## Components

### Masthead
- **Header strip:** ink ground, paper text, strip label type, "n/8 · page name" left (truncates with ellipsis), language switch right. Current language underlined 2px in paper; the other at 75% opacity, full on hover.
- **Range bar:** 0.7rem tall, five flex segments sized from the virtual patient's bins, very low and low striped. Decorative (aria-hidden); the range key under it says the numbers in text.
- **Range key:** 0.8125rem centred line, in-range share in bold, hairline under it.

### Navigation
- **Chapter strip:** eight equal numbered tabs, 600 0.8125rem, hairline dividers, ink rule under the strip. Hover fills band green; the current chapter is an inverted ink tab; the stopped-at chapter carries the corner triangle. Chapter names are screen-reader text and title tooltips.
- **Prev/next (turn):** two-cell grid with a 1px ink frame and ink gutter, 700 links with SVG arrows, next aligned right. Hover fills band green.

### Report Panel (signature)
- **Frame:** 1px ink rule, square, white.
- **Title:** panel label in tracked caps, caption subtitle in grey under it, ink rule below.
- **Chart:** band green 70 to 180 rect, darkened high rule at 180 and low red at 70 (2px), hairline grid, ink axis, mg/dL ticks with mmol/L sub-ticks, spread greys behind, lines in ink (heavy median; others dashed, dotted or grey), meal ticks with grams, open dots in paper with ink stroke.
- **Legend:** inline line samples (28x10 SVG) with names, wraps.
- **TIR bars:** per-case range bars (0.9rem) with labels.
- **Show the numbers:** a details element, grey 600 summary, hairline above, holding the data table.
- **Provenance line:** hairline above, grey 0.75rem tracked caps "virtual patient · model output".

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
- **Don't** add decorative icons; the only icons are functional SVG arrows on prev/next.
- **Don't** use tracked uppercase outside report furniture (strip, panel title, table head, provenance line).
- **Don't** use the high yellow (#f9d648) as a line or text colour on white; use the darkened high rule.
