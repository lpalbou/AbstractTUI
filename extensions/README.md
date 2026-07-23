# AbstractTUI extensions — the sibling-crate family

Genuinely new domains ship as `abstracttui-<domain>` sibling crates in
this directory (residents: `abstracttui-graph`, `abstracttui-mermaid`),
governed by
[ADR-0004](../docs/adr/0004-extension-packaging.md): public API only —
a needed-but-missing capability is a core backlog item, never a private
hook; the core dependency posture binds siblings (parsers are
hand-rolled, new dependencies take the same review path as core); and
every crate spells its core dependency dual-form —
`abstracttui = { version = "0.2", path = "../.." }` — so in-repo
workspace builds ride core HEAD while crates.io publishes resolve the
published version. The root `Cargo.toml` picks this directory up as
workspace members (`extensions/*`), which is how family crates ride CI
against core HEAD; publish order is core first, family the same day,
and each ADR-0001 breaking budget includes the family migration.
