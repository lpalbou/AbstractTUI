//! Deterministic pseudo-random byte-soup generators — the ammunition for
//! parser fuzz ("never panics on any byte sequence") without external
//! fuzzing infrastructure.
//!
//! OWNER: REDTEAM.
//!
//! Everything is seeded and reproducible: a failing case is re-run by
//! seed, and CI runs the same corpus every time. Structured generators
//! bias the soup toward the shapes that actually break parsers —
//! sequence-like prefixes, truncations, giant params, split UTF-8 —
//! because uniform random bytes almost never explore deep parser states.

/// xorshift64* PRNG. Tiny, fast, no dependencies, stable across platforms.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // Zero state would lock xorshift at zero forever.
        Rng(if seed == 0 { 0x9e3779b97f4a7c15 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    pub fn byte(&mut self) -> u8 {
        (self.next_u64() >> 56) as u8
    }

    /// Uniform in [0, n). n = 0 returns 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Uniform in [lo, hi] inclusive.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi.saturating_sub(lo) + 1)
    }

    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.next_u32() % den < num
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Uniform random bytes — the baseline soup.
pub fn random_chunk(rng: &mut Rng, max_len: usize) -> Vec<u8> {
    let len = rng.below(max_len + 1);
    (0..len).map(|_| rng.byte()).collect()
}

/// A plausible-but-random escape sequence: `ESC` + intro + params + final,
/// with deliberate rule-breaking (missing final, garbage params, giant
/// params) at a controlled rate.
pub fn sequence_shaped(rng: &mut Rng) -> Vec<u8> {
    let mut out = vec![0x1b];
    match rng.below(6) {
        // CSI with 0-6 params.
        0 | 1 => {
            out.push(b'[');
            if rng.chance(1, 4) {
                out.push(*rng.pick(b"?><="));
            }
            let nparams = rng.below(7);
            for i in 0..nparams {
                if i > 0 {
                    out.push(*rng.pick(b";;:"));
                }
                push_number(rng, &mut out);
            }
            if rng.chance(1, 8) {
                out.push(*rng.pick(b" !\"#$%&'"));
            }
            // 1-in-6: drop the final byte (truncated sequence).
            if !rng.chance(1, 6) {
                out.push(rng.range(0x40, 0x7e) as u8);
            }
        }
        // OSC, sometimes unterminated, terminated by BEL or ST.
        2 => {
            out.push(b']');
            push_number(rng, &mut out);
            out.push(b';');
            for _ in 0..rng.below(24) {
                out.push(rng.range(0x20, 0x7e) as u8);
            }
            match rng.below(3) {
                0 => out.push(0x07),
                1 => out.extend_from_slice(b"\x1b\\"),
                _ => {} // left open
            }
        }
        // DCS / APC / PM / SOS frame.
        3 => {
            out.push(*rng.pick(b"P_^X"));
            for _ in 0..rng.below(24) {
                out.push(rng.byte());
            }
            if !rng.chance(1, 3) {
                out.extend_from_slice(b"\x1b\\");
            }
        }
        // SS3 key.
        4 => {
            out.push(b'O');
            out.push(rng.range(0x40, 0x7e) as u8);
        }
        // Bare ESC + random byte (Alt-prefixed / charset / unknown).
        _ => out.push(rng.byte()),
    }
    out
}

fn push_number(rng: &mut Rng, out: &mut Vec<u8>) {
    match rng.below(8) {
        // Giant param: tests saturating parse + length caps.
        0 => out.extend_from_slice(
            format!("{}", (rng.next_u32() as u64).saturating_mul(1 << 20)).as_bytes(),
        ),
        // Absurdly long digit run.
        1 => {
            for _ in 0..rng.range(30, 300) {
                out.push(b'0' + (rng.below(10) as u8));
            }
        }
        _ => out.extend_from_slice(format!("{}", rng.below(10000)).as_bytes()),
    }
}

/// Valid UTF-8 text (mixing ASCII, accents, CJK wide, emoji, combining),
/// then truncated mid-encode at a random point — the classic split-read.
pub fn truncated_utf8(rng: &mut Rng) -> Vec<u8> {
    const POOL: &[&str] = &[
        "hello",
        "héllo",
        "café",
        "日本語",
        "中文字",
        "한국어",
        "🎉",
        "🧪",
        "👍🏽",
        "e\u{301}",
        "a\u{300}\u{316}",
        "Ω≈ç√",
        "𝔘𝔫𝔦",
    ];
    let mut s = String::new();
    for _ in 0..rng.range(1, 5) {
        // Turbofish pins `T = &str`: without it, older rustc (≤1.87, the
        // declared MSRV) back-propagates `push_str`'s `&str` expectation
        // into `T = str` and rejects the call — an inference accident,
        // not a feature need.
        s.push_str(rng.pick::<&str>(POOL));
    }
    let mut bytes = s.into_bytes();
    if rng.chance(2, 3) && !bytes.is_empty() {
        // Cut anywhere, including mid-codepoint.
        let cut = rng.range(1, bytes.len());
        bytes.truncate(cut);
    }
    bytes
}

/// The rig's standard hostile corpus: `count` chunks mixing all
/// generators, plus deterministic nasty edge cases up front.
pub fn hostile_corpus(seed: u64, count: usize) -> Vec<Vec<u8>> {
    let mut rng = Rng::new(seed);
    let mut corpus: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"\x1b".to_vec(),
        b"\x1b\x1b\x1b".to_vec(),
        b"\x1b[".to_vec(),
        b"\x1b[;;;;;;;;;;;;;".to_vec(),
        b"\x1b[999999999999999999999999H".to_vec(),
        b"\x1b[38;2;300;300;300m".to_vec(),
        b"\x1b[38;5m".to_vec(),
        b"\x1b[38m".to_vec(),
        b"\x1b]8;;".to_vec(),
        b"\x1b]0;title never ends".to_vec(),
        b"\x1bP+q544e\x1b".to_vec(),
        b"\xff\xfe\xfd".to_vec(),
        b"\xc3".to_vec(),         // lone UTF-8 lead
        b"\xe2\x82".to_vec(),     // 2/3 of €
        b"\xf0\x9f\x8e".to_vec(), // 3/4 of an emoji
        b"\x80\x80\x80".to_vec(), // stray continuations
        b"\xed\xa0\x80".to_vec(), // surrogate half
        b"\xc0\xaf".to_vec(),     // overlong '/'
        b"\x18\x1a".to_vec(),     // CAN/SUB
        vec![0x1b; 512],
        vec![b'A'; 4096],
    ];
    while corpus.len() < count {
        let chunk = match rng.below(4) {
            0 => random_chunk(&mut rng, 64),
            1 | 2 => sequence_shaped(&mut rng),
            _ => truncated_utf8(&mut rng),
        };
        corpus.push(chunk);
    }
    corpus
}

/// Split one byte stream into random-sized sub-chunks (1..=max) — for
/// testing that incremental feeding is equivalent to one-shot feeding.
pub fn random_splits(rng: &mut Rng, bytes: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let n = rng.range(1, max.max(1)).min(bytes.len() - i);
        out.push(bytes[i..i + n].to_vec());
        i += n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let a: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b);
        let c: Vec<u64> = {
            let mut r = Rng::new(43);
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_ne!(a, c);
    }

    #[test]
    fn zero_seed_does_not_lock() {
        let mut r = Rng::new(0);
        assert_ne!(r.next_u64(), 0);
    }

    #[test]
    fn corpus_is_reproducible_and_sized() {
        let a = hostile_corpus(7, 100);
        let b = hostile_corpus(7, 100);
        assert_eq!(a, b);
        assert_eq!(a.len(), 100);
    }

    #[test]
    fn splits_reassemble() {
        let mut rng = Rng::new(9);
        let data: Vec<u8> = (0..=255).collect();
        let parts = random_splits(&mut rng, &data, 7);
        let joined: Vec<u8> = parts.concat();
        assert_eq!(joined, data);
        assert!(parts.iter().all(|p| !p.is_empty() && p.len() <= 7));
    }
}
