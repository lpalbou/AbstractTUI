//! Surface diagnostics: the structural oracle (RT1-4) and the debug
//! renderer. Split from `surface.rs` for the file-size budget; everything
//! here reads through the public surface API — no privileged access, so
//! the oracle can never drift from what real consumers see.

use super::surface::Surface;

impl Surface {
    /// Structural oracle for property tests (RT1-4): checks every
    /// invariant the pipeline leans on and names the first violation.
    /// Available in ALL builds: the old `cfg(any(test, debug_assertions))`
    /// gate made every integration test calling it fail to COMPILE under
    /// `cargo test --release` — `cfg(test)` applies only to the library's
    /// own unit-test build, never to external test binaries linking the
    /// release lib (caught live by the scheduled perf workflow's release
    /// prebuild, 2026-07-23). Costs nothing unless called.
    ///
    /// Checks: wide pairs intact (leader immediately followed by a
    /// continuation mirroring its full style, `ul` included; no orphan
    /// continuation; no leader in the last column), pooled glyph ids
    /// resolve in THIS surface's pool, link ids resolve in THIS surface's
    /// table.
    pub fn debug_validate(&self) -> Result<(), String> {
        for y in 0..self.height() {
            let mut x = 0;
            while x < self.width() {
                let c = *self.get(x, y).expect("in bounds");
                if c.is_continuation() {
                    return Err(format!("orphan continuation at ({x},{y})"));
                }
                if c.glyph.is_pooled() && self.pool().get(c.glyph.pool_id()).is_none() {
                    return Err(format!(
                        "pool id {} out of range at ({x},{y})",
                        c.glyph.pool_id()
                    ));
                }
                if c.link != 0 && self.link_uri(c.link).is_none() {
                    return Err(format!("link id {} out of range at ({x},{y})", c.link));
                }
                if c.is_wide_leader() {
                    if x + 1 >= self.width() {
                        return Err(format!("wide leader in last column at ({x},{y})"));
                    }
                    let cont = *self.get(x + 1, y).expect("in bounds");
                    if !cont.is_continuation() {
                        return Err(format!("leader without continuation at ({x},{y})"));
                    }
                    if (cont.fg, cont.bg, cont.ul, cont.attrs, cont.link)
                        != (c.fg, c.bg, c.ul, c.attrs, c.link)
                    {
                        return Err(format!(
                            "continuation style diverges from leader at ({x},{y})"
                        ));
                    }
                    x += 2;
                } else {
                    x += 1;
                }
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for Surface {
    /// Renders rows as plain text (pool-resolved, EMPTY as space) — the
    /// assertion currency of surface tests.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Surface {}x{}", self.width(), self.height())?;
        for y in 0..self.height() {
            let mut line = String::new();
            for x in 0..self.width() {
                let c = self.get(x, y).expect("in bounds");
                if c.is_continuation() {
                    continue; // the leader renders both columns
                }
                let s = self.glyph_str(c);
                line.push_str(if s.is_empty() { " " } else { s });
            }
            writeln!(f, "|{line}|")?;
        }
        Ok(())
    }
}
