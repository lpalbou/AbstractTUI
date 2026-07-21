//! Kitty graphics protocol MODEL: parses APC `_G` frames out of a byte
//! stream and tracks image lifecycle state (transmitted / placed /
//! deleted) — the referee for GFX3D's ImageSession accounting
//! (transmit-once, re-place on move, delete on drop, no leaks).
//!
//! OWNER: REDTEAM.
//!
//! Scope: control-data parsing and id accounting, NOT pixel decoding.
//! Payload chunks are length-checked (≤ 4096, non-final chunks 4-aligned
//! per the spec) and reassembled per transmission so chunked transmits
//! count once. tmux passthrough frames (`ESC P tmux; ... ESC \` with
//! inner ESCs doubled) unwrap transparently when enabled.

use std::collections::BTreeMap;

/// One parsed APC `_G` frame: control k=v pairs + payload bytes.
#[derive(Clone, Debug)]
pub struct KittyFrame {
    pub keys: Vec<(String, String)>,
    pub payload: Vec<u8>,
}

impl KittyFrame {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn u32_key(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(|v| v.parse().ok())
    }
}

/// Lifecycle state per image id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageState {
    /// Completed transmissions (a=t/T with final chunk m=0).
    pub transmits: u32,
    /// Live placements (a=p or a=T; delete removes them).
    pub placements: u32,
    /// Data deleted (uppercase delete verbs free data).
    pub data_freed: bool,
}

/// The model. Feed it the exact bytes a terminal would receive.
#[derive(Default)]
pub struct KittyModel {
    images: BTreeMap<u32, ImageState>,
    /// Mid-transmission chunk state: id -> accumulated payload.
    open_chunks: Option<(u32, Vec<u8>)>,
    /// Protocol violations (chunk rules, place-before-transmit, id 0...).
    pub violations: Vec<String>,
    /// Non-kitty bytes are counted, not stored (the presenter's cell
    /// traffic legitimately interleaves).
    pub other_bytes: usize,
    /// Unwrap tmux passthrough (`ESC P tmux; ... ESC \\`, inner ESC
    /// doubled) before parsing.
    pub tmux_unwrap: bool,
}

impl KittyModel {
    pub fn new() -> KittyModel {
        KittyModel::default()
    }

    pub fn with_tmux_unwrap() -> KittyModel {
        KittyModel {
            tmux_unwrap: true,
            ..KittyModel::default()
        }
    }

    pub fn image(&self, id: u32) -> Option<&ImageState> {
        self.images.get(&id)
    }

    /// Ids with transmitted data not yet freed — the LEAK set at
    /// end-of-session.
    pub fn live_data_ids(&self) -> Vec<u32> {
        self.images
            .iter()
            .filter(|(_, s)| s.transmits > 0 && !s.data_freed)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Ids with live placements.
    pub fn placed_ids(&self) -> Vec<u32> {
        self.images
            .iter()
            .filter(|(_, s)| s.placements > 0)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Total completed transmissions of `id` — "transmit once" means
    /// this stays 1 however often the image moves.
    pub fn transmit_count(&self, id: u32) -> u32 {
        self.images.get(&id).map(|s| s.transmits).unwrap_or(0)
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let unwrapped;
        let mut data = bytes;
        if self.tmux_unwrap {
            unwrapped = unwrap_tmux(bytes);
            data = &unwrapped;
        }
        let mut i = 0;
        while i < data.len() {
            if data[i] == 0x1b && data.get(i + 1) == Some(&b'_') && data.get(i + 2) == Some(&b'G') {
                let end = data[i + 3..]
                    .windows(2)
                    .position(|w| w == b"\x1b\\")
                    .map(|p| i + 3 + p);
                let Some(end) = end else {
                    self.violations.push("unterminated APC frame".into());
                    return;
                };
                self.parse_frame(&data[i + 3..end]);
                i = end + 2;
            } else {
                self.other_bytes += 1;
                i += 1;
            }
        }
    }

    fn parse_frame(&mut self, body: &[u8]) {
        let semi = body.iter().position(|&b| b == b';');
        let (ctrl, payload) = match semi {
            Some(p) => (&body[..p], body[p + 1..].to_vec()),
            None => (body, Vec::new()),
        };
        let ctrl = String::from_utf8_lossy(ctrl);
        let mut keys = Vec::new();
        for kv in ctrl.split(',').filter(|s| !s.is_empty()) {
            match kv.split_once('=') {
                Some((k, v)) => keys.push((k.to_string(), v.to_string())),
                None => self
                    .violations
                    .push(format!("malformed control item {kv:?}")),
            }
        }
        let frame = KittyFrame { keys, payload };

        // Chunking rules apply to every payload-bearing frame.
        if frame.payload.len() > 4096 {
            self.violations.push(format!(
                "chunk of {} bytes exceeds 4096",
                frame.payload.len()
            ));
        }
        let m = frame.get("m").unwrap_or("0");
        let is_continuation = frame.get("a").is_none() && self.open_chunks.is_some();

        if let Some((open_id, ref mut acc)) = self.open_chunks {
            if !is_continuation {
                self.violations.push(format!(
                    "new command while transmission of image {open_id} is open"
                ));
                self.open_chunks = None;
            } else {
                if m == "1" && !frame.payload.len().is_multiple_of(4) {
                    self.violations.push(format!(
                        "non-final chunk not 4-aligned ({})",
                        frame.payload.len()
                    ));
                }
                acc.extend_from_slice(&frame.payload);
                if m == "0" {
                    let id = open_id;
                    self.open_chunks = None;
                    self.finish_transmit(id);
                }
                return;
            }
        }

        match frame.get("a").unwrap_or("t") {
            "t" | "T" => {
                let Some(id) = frame.u32_key("i") else {
                    self.violations
                        .push("transmit without an id (undeletable)".into());
                    return;
                };
                if id == 0 {
                    self.violations
                        .push("transmit with id 0 (terminal-chosen)".into());
                    return;
                }
                if m == "1" {
                    if !frame.payload.len().is_multiple_of(4) {
                        self.violations.push(format!(
                            "non-final chunk not 4-aligned ({})",
                            frame.payload.len()
                        ));
                    }
                    self.open_chunks = Some((id, frame.payload.clone()));
                } else {
                    self.finish_transmit(id);
                }
                if frame.get("a") == Some("T") {
                    self.images.entry(id).or_default().placements += 1;
                }
            }
            "p" => {
                let Some(id) = frame.u32_key("i") else {
                    self.violations.push("place without an id".into());
                    return;
                };
                let st = self.images.entry(id).or_default();
                if st.transmits == 0 {
                    self.violations
                        .push(format!("place of untransmitted image {id}"));
                }
                st.placements += 1;
            }
            "d" => {
                let verb = frame.get("d").unwrap_or("a");
                let frees = verb.chars().any(|c| c.is_ascii_uppercase());
                match verb.to_ascii_lowercase().as_str() {
                    "i" => {
                        if let Some(id) = frame.u32_key("i") {
                            let st = self.images.entry(id).or_default();
                            st.placements = 0;
                            if frees {
                                st.data_freed = true;
                            }
                        }
                    }
                    "a" => {
                        for st in self.images.values_mut() {
                            st.placements = 0;
                            if frees {
                                st.data_freed = true;
                            }
                        }
                    }
                    other => self.violations.push(format!(
                        "delete verb {other:?} not modeled (extend the rig)"
                    )),
                }
            }
            "q" => {} // queries are stateless
            other => self
                .violations
                .push(format!("action {other:?} not modeled")),
        }
    }

    fn finish_transmit(&mut self, id: u32) {
        let st = self.images.entry(id).or_default();
        st.transmits += 1;
        st.data_freed = false;
    }
}

/// Unwrap tmux passthrough: `ESC P tmux; <inner, ESC doubled> ESC \` —
/// returns the concatenated inner bytes plus everything outside
/// passthrough frames verbatim.
pub fn unwrap_tmux(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    const HEAD: &[u8] = b"\x1bPtmux;";
    while i < bytes.len() {
        if bytes[i..].starts_with(HEAD) {
            let body_start = i + HEAD.len();
            // Find the terminating ST: a LONE ESC followed by '\'
            // (doubled ESCs belong to the inner payload).
            let mut j = body_start;
            let mut inner = Vec::new();
            let mut terminated = false;
            while j < bytes.len() {
                if bytes[j] == 0x1b {
                    if bytes.get(j + 1) == Some(&0x1b) {
                        inner.push(0x1b);
                        j += 2;
                        continue;
                    }
                    if bytes.get(j + 1) == Some(&b'\\') {
                        terminated = true;
                        j += 2;
                        break;
                    }
                }
                inner.push(bytes[j]);
                j += 1;
            }
            if !terminated {
                // Truncated wrapper: surface the raw tail (never lose bytes).
                out.extend_from_slice(&bytes[i..]);
                return out;
            }
            out.extend_from_slice(&inner);
            i = j;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmit_place_delete_accounting() {
        let mut m = KittyModel::new();
        m.feed(b"\x1b_Gi=7,a=T,f=24,s=1,v=1;QUJD\x1b\\");
        assert_eq!(m.transmit_count(7), 1);
        assert_eq!(m.image(7).unwrap().placements, 1);
        m.feed(b"\x1b_Gi=7,a=p\x1b\\");
        assert_eq!(m.image(7).unwrap().placements, 2);
        m.feed(b"\x1b_Ga=d,d=I,i=7\x1b\\");
        assert_eq!(m.image(7).unwrap().placements, 0);
        assert!(m.image(7).unwrap().data_freed);
        assert!(m.live_data_ids().is_empty());
        assert!(m.violations.is_empty(), "{:?}", m.violations);
    }

    #[test]
    fn chunked_transmit_counts_once_and_checks_alignment() {
        let mut m = KittyModel::new();
        m.feed(b"\x1b_Gi=9,a=t,f=24,m=1;AAAA\x1b\\");
        m.feed(b"\x1b_Gm=1;BBBB\x1b\\");
        m.feed(b"\x1b_Gm=0;CC\x1b\\");
        assert_eq!(m.transmit_count(9), 1);
        assert!(m.violations.is_empty(), "{:?}", m.violations);
        // Misaligned non-final chunk is a violation.
        let mut bad = KittyModel::new();
        bad.feed(b"\x1b_Gi=3,a=t,m=1;AAA\x1b\\");
        assert!(!bad.violations.is_empty());
    }

    #[test]
    fn place_before_transmit_is_a_violation() {
        let mut m = KittyModel::new();
        m.feed(b"\x1b_Gi=5,a=p\x1b\\");
        assert!(m.violations.iter().any(|v| v.contains("untransmitted")));
    }

    #[test]
    fn tmux_unwrap_round_trip() {
        let inner = b"\x1b_Gi=4,a=T;QQ==\x1b\\".to_vec();
        // Wrap: double every ESC, frame with ESC P tmux; ... ESC \.
        let mut wrapped = b"\x1bPtmux;".to_vec();
        for &b in &inner {
            if b == 0x1b {
                wrapped.push(0x1b);
            }
            wrapped.push(b);
        }
        wrapped.extend_from_slice(b"\x1b\\");
        assert_eq!(unwrap_tmux(&wrapped), inner, "unwrap must be byte-exact");
        let mut m = KittyModel::with_tmux_unwrap();
        m.feed(&wrapped);
        assert_eq!(m.transmit_count(4), 1);
        assert!(m.violations.is_empty(), "{:?}", m.violations);
    }

    #[test]
    fn interleaved_cell_traffic_is_counted_not_confused() {
        let mut m = KittyModel::new();
        m.feed(b"\x1b[2J\x1b[1;1Hhello \x1b_Gi=2,a=T;AA==\x1b\\ world");
        assert_eq!(m.transmit_count(2), 1);
        assert!(m.other_bytes > 10);
        assert!(m.violations.is_empty());
    }
}
