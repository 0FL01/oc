# Goal: OC2 exploration group parity in the TUI transcript

Status: complete
Source: owner bug report with screenshots and approved plan, 2026-10-18
(«Эти все блоки должны быть автоматически спрятаны под спойлер … гляди на цвета и
расстояние между строк»; owner chose upstream parity for failed/denied/truncated
members and OC2 styling for standalone inline tools).
Task: T70 / M8

## Required Outcomes

- R1: every adjacent Read/Glob/Grep/WebFetch run collapses under one muted
  `→ Explored: N reads, M searches` header (`Exploring:` while running), as in
  `.local/opencode/packages/tui/src/routes/session/group-view.tsx:109-156` and
  `grouping/session.ts:77-90`. Truncated, failed, denied and cancelled members join
  the group; only `unknown`, permission-pending and argument-stream rows stay outside.
  Reasoning, text and other tools still split runs.
- R2: click toggles the group; expanded members are listed directly below the header
  without blank lines between them, each with its own OC2 icon (`→` Read, `✱`
  Glob/Grep, `%` WebFetch) in muted color, failed in error color with its error text.
- R3: standalone Read/Glob/Grep/WebFetch/Skill inline rows use the same OC2 icon and
  muted color (`index.tsx:2635-2680,3025-3105`); warning/error/strike states remain.

## Constraints

No storage schema or migration change; full/visible/position renderers stay in parity;
hover/hit and expansion anchoring remain correct.
