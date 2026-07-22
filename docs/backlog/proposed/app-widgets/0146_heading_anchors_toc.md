# 0146 — Heading anchors + TOC extraction from markdown

- Status: proposed
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: none; 0165 (hyperlink hit-testing) consumes the anchor ids

## Problem

A document reader needs "jump to section" (TOC sidebar, `#anchor`
links); the md pipeline typesets headings but exposes no structure.

## What we want to do

(1) `md::outline(source) -> Vec<Heading { level, text, anchor_id, row }>`
— rows computed from the typeset result at a given width so a TOC can
scroll-to; (2) stable anchor-id slugging (GitHub-compatible:
lowercase, dashes, dedup suffixes); (3) intra-document link targets:
`[text](#anchor)` resolves to a row; activation rides 0165 when it
lands (until then, apps consume the outline directly for TOC lists).

## Validation

Slug golden table (unicode, dedup, punctuation); outline rows match
typeset rows across widths (property test: re-wrap then re-outline);
anchor links resolve.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).
