//! Hand-rolled timing harness for the perf budgets in the vision charter
//! — no criterion (dependency policy), no statistics theater: warmup,
//! N timed runs of K iterations each, report the MEDIAN run, assert
//! against a budget only inside `#[ignore]`d perf tests that are run
//! explicitly (`cargo test --release -- --ignored perf_`).
//!
//! OWNER: REDTEAM.
//!
//! Median over mean: one OS scheduling hiccup must not fail a build.
//! Budgets always carry slack vs the charter number (charter is the
//! product truth on the reference machine; CI machines vary) — the test
//! names the charter figure in its assert message so drift is visible.

use std::time::{Duration, Instant};

/// Result of one measurement: per-iteration medians and extremes.
#[derive(Clone, Debug)]
pub struct Measurement {
    pub name: String,
    /// Median run's per-iteration time.
    pub median: Duration,
    /// Fastest run's per-iteration time.
    pub best: Duration,
    /// Slowest run's per-iteration time.
    pub worst: Duration,
    pub runs: usize,
    pub iters_per_run: usize,
}

impl Measurement {
    /// Human line for `--nocapture` output.
    pub fn report(&self) -> String {
        format!(
            "{}: median {:?} (best {:?}, worst {:?}) over {} runs x {} iters",
            self.name, self.median, self.best, self.worst, self.runs, self.iters_per_run
        )
    }

    /// Assert the median per-iteration time is under `budget`. Panics with
    /// the full report so a red perf test is self-explaining.
    pub fn assert_under(&self, budget: Duration) {
        assert!(
            self.median <= budget,
            "PERF BUDGET EXCEEDED: {} (budget {:?})",
            self.report(),
            budget
        );
    }
}

/// Time `f` as `runs` runs of `iters` iterations (after `warmup`
/// iterations). `f` receives the iteration index; use it to vary inputs
/// so the optimizer cannot hoist the work out of the loop.
pub fn time_median<F: FnMut(usize)>(
    name: &str,
    warmup: usize,
    runs: usize,
    iters: usize,
    mut f: F,
) -> Measurement {
    assert!(
        runs >= 1 && iters >= 1,
        "need at least one run and one iter"
    );
    for i in 0..warmup {
        f(i);
    }
    let mut per_iter: Vec<Duration> = Vec::with_capacity(runs);
    for r in 0..runs {
        let start = Instant::now();
        for i in 0..iters {
            f(r * iters + i);
        }
        per_iter.push(start.elapsed() / iters as u32);
    }
    per_iter.sort();
    Measurement {
        name: name.to_string(),
        median: per_iter[per_iter.len() / 2],
        best: per_iter[0],
        worst: *per_iter.last().expect("runs >= 1"),
        runs,
        iters_per_run: iters,
    }
}

/// Defeat dead-code elimination without `unsafe` or volatile: route the
/// value through a black-box sink the optimizer cannot see through.
/// (`std::hint::black_box` is stable since 1.66 — prefer it; this wrapper
/// exists so call sites read as intent.)
pub fn sink<T>(value: T) -> T {
    std::hint::black_box(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_is_computed() {
        let mut n = 0u64;
        let m = time_median("noop", 2, 5, 100, |i| {
            n = n.wrapping_add(i as u64);
            sink(n);
        });
        assert_eq!(m.runs, 5);
        assert!(m.best <= m.median && m.median <= m.worst);
    }

    #[test]
    #[should_panic(expected = "PERF BUDGET EXCEEDED")]
    fn budget_violation_panics() {
        let m = time_median("sleepy", 0, 1, 1, |_| {
            std::thread::sleep(Duration::from_millis(2));
        });
        m.assert_under(Duration::from_nanos(1));
    }
}
