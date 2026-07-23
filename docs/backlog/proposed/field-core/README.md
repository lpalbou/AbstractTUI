# field-core — findings from the abstractcore-console build

Feedback band for the third-wave validator app `abstractcore-console`
(the AbstractCore configuration console,
`~/tmp/abstractframework/abstractcore/console-tui` — launched
2026-07-25 on abstracttui 0.2.22): reproduced engine defects and gaps
with field workarounds to delete, in the house grammar of
`field-gateway`/`field-agora`.

- **Number range: 1100-1190.** (0800-0890 = field-agora, which
  overflowed into 0895-0910; 0900-0990 = field-gateway, which
  overflowed into 1000-1050. Overflow rule stated up front this time:
  past 1190, continue at the next free fifty and note it here.)
- One file per finding: engine `file:line` evidence, severity, the
  app-side workaround shipped meanwhile, what the engine should own.
- Findings about AbstractCore itself (the Python side) do NOT belong
  here — they go to the core seat via the app's reports.

## Findings

| # | Title | Severity | Class | Status |
| --- | --- | --- | --- | --- |
| 1100 | TextInput: no cursor-at-end / select-all on open — prefilled editors insert at position 0 | P3 | API gap | proposed |
