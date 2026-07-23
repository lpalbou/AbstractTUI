# ADR-0004: Extension packaging — feature classes, sibling-crate family, dependency-posture inheritance

## Status

Accepted (maintainer approval 2026-07-23; proposed by backlog 0400,
drafted by the extensions study 2026-07-22).

## Context

The modularity brief: keep the default package lean — extensions are
installed only when needed. The engine ships one crate with zero cargo
features and a five-crate dependency posture. The decision queue that
forced this ruling: heavy in-tree modules (3D, JPEG, protocol encoders —
backlog 0410), the control server (0320), the diagram domains
(node-graph 0430/0440, mermaid 0450), app-kit controls (band 0500), and
the readable-HTML slice (0470).

## Decision

1. **Two cargo feature classes.**
   - *Default-ON trim features* for heavy, severable, in-tree code:
     `three`, `jpeg`, `proto` (backlog 0410). Out-of-box behavior is
     unchanged; `default-features = false` is the documented opt-out
     for minimal builds.
   - *Default-OFF opt-in features* for in-tree capability a minimal app
     must not silently carry: `control-server` (0320) is the first —
     feature-off means the constructor does not exist (compile-time
     absence as the security posture).
2. **Additivity rule (both classes).** Enabling a feature may ADD items
   (types, constructors, prelude re-exports) and may make a runtime
   path SUCCEED where it previously failed with a named error; it must
   never change the semantics, defaults, or output of code that
   compiled without it. Feature-off runtime seams degrade with NAMED
   errors/labels — never silently.
3. **Sibling-crate family.** Genuinely new domains ship as
   `abstracttui-<domain>` crates (first candidates: `abstracttui-graph`,
   `abstracttui-mermaid`): public API only — a needed-but-missing
   capability is a core backlog item, never a private hook; an in-repo
   cargo workspace so CI builds the family against core HEAD; dual-form
   dependency spelling (`abstracttui = { version = "0.x", path =
   "../.." }`); publish order core-first, family the same day; every
   ADR-0001 breaking budget includes the family migration.
4. **Dependency posture: siblings inherit the spirit.** Allowed by
   default: std, `abstracttui` itself, and the core's five small crates
   where already justified; parsers are hand-rolled (mermaid, HTML —
   the same discipline as the in-crate JSON/PNG/JPEG decoders). Any new
   dependency takes the same review-note path as core. Named exception
   window, NOT granted here: TLS/network-class needs cannot responsibly
   be hand-rolled — that exception is decided by live-data 0050's
   transport ADR, and any extension needing it waits for that ruling.
5. **The anchor surface** (what extensions may build on):
   `ui::Element` + draw closures, `StyledCanvas` (including the
   link-registration seam when landed), `layout` styles, `reactive`
   signals, `theme::TokenSet`, `app::Overlays` + the anchored-popup
   primitive, the canvas/vector layer (0420) when landed, and the
   control bus (0310) when public. The list is exhaustive; it grows
   only by core backlog item.
6. **Classification rule.** "Does a minimal app pay for it in-tree, and
   does it have its own release cadence?" Both yes = sibling crate.
   Cost-yes/cadence-no = feature (ON if trim, OFF if opt-in surface).
   Neither = core. Recorded dry-run: control bus = core; control server
   = default-OFF feature; MCP bridge + attach client = out-of-crate;
   app-kit choice controls/forms = core; graph + mermaid = siblings;
   canvas layer + link seam = core.
7. **Non-goals.** No dynamic loading or ABI plugins; no scripting
   runtime; no discovery machinery beyond crates.io naming; no
   behavior-changing features; no fork of the token/theming discipline.

## Consequences

- Minimal apps can trim the heavy built-ins without losing correctness;
  nothing changes out of the box until a feature is toggled.
- The diagram lane (canvas → graph widgets → mermaid) can start: 0420
  lands in core, 0430/0440/0450 as sibling crates under this contract.
- Feature matrices join CI when the first feature lands (0410's
  execution); the semver gate covers the family once published.
