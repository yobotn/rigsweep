# 0004: The family look, no "AI" aesthetics

- **Status:** accepted
- **Date:** 2026-11-23
- **Deciders:** @yobotn

## Context

RigSweep started with a broom emoji on a blue-to-green gradient, and an optional "AI-ish" style (gradients,
glow, emoji) in the app and on the site. It read as generic AI branding and didn't match the maintainer's other
apps, Focuside and KeepRun.

## Decision

One flat look shared with Focuside and KeepRun: zinc `#18181b` tile, a geometric white mark, one accent
`#22c55e` (green buttons carry dark `#052e16` text). The mark is a square-cut R with a green square
(`brand/assets/mark.svg`; `mark-small.svg` at 32 px and below). No gradients, no glow, no style switch, no
emoji as the logo or in headings. Blue stays only for information (the CPU chart). UI icons are Lucide.

The AI-agent *substance* stays: the features, the "Room for your agents" line, `llms.txt`, the CLI and MCP
server.

## Consequences

- `npm run brand` renders every icon, the favicon and the social card from the mark.
- The 🧹 emoji only labels the cleanup feature, never the brand.

## Challenge log

- *"Keep the gradient on the homepage headline?"* It was kept for a day, then dropped with the rest of the
  AI look on 2026-11-23.
