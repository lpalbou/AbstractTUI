# Logical content selection preserves source provenance

Status: Accepted

## Context

Screen-text copying extracts composed terminal glyphs. That remains useful for arbitrary
widgets, but loses Markdown source offsets, paragraph boundaries, table identity and code
structure. Applications that explain or act on a passage need the logical source that produced
the selected cells. Reconstructing it from glyphs fails after wrapping, clipping and formatting.

## Decision

Keep logical selection opt-in and separate from screen copying. Emit source provenance while
parsing, then carry it through the same wrapping and table layout used for painting. Store
private row sidecars rather than add required fields to public rich-text spans.

Expose source hits and retained selections with an immutable source snapshot, producer/item
identity, source range, content kind and applicable table/code context. Validate UTF-8 ranges.
A selection cannot silently change meaning when its producer replaces the source. Preserve it
through pure layout changes when that snapshot remains current; clear it when its owner/source
is invalidated, the owning view closes, or the application changes its conversation.

The content binding owns drags only on mapped content. Links, buttons, scrollbars and unmapped
custom rows retain their established routing and screen-copy fallback. Cancellation releases
pointer capture. A scoped follow-tail hold composes with existing screen/manual freezes so text
does not move under a selection gesture.

The library reports selected data and supplies highlight/lifecycle mechanics. The host owns
Explain, compose, send, steer, file access and tool permissions. Selection or pointer gaze
must never submit instructions, run tools, grant access or read the clipboard automatically.
Hosts bound staged context, preserve drafts and explicitly label omitted surrounding context.

## Alternatives and consequences

- Reading the flattened screen is retained for generic copy, but is not logical provenance.
- Adding mandatory source fields to public `Span` literals would break unrelated consumers.
- Keeping mappings in each application duplicates parsing/layout and produces inconsistent hits.

Opt-in mapping adds source metadata and work on first interaction. Keep ordinary rendering free
of logical-mapping callbacks when the feature is unused, and share column/wrap solvers so future
rendering changes cannot silently separate pixels from source hits.

This extends [ADR 0005](0005-content-rendering-responsibilities.md). The remaining
logical-selection scope is tracked by [0160](../backlog/proposed/app-widgets/0160_content_selection_copy.md).
