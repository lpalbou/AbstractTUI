//! REDTEAM cycle-2 attack: pixel-protocol emitter byte shapes (kitty /
//! sixel / iTerm2) — structural validators, not full decoders: framing,
//! chunking and range rules the protocols mandate. Split from
//! adv_gfx.rs (file-size discipline); the parser attacks live there.

use abstracttui::base::Rgba;
use abstracttui::gfx::bitmap::Bitmap;

// ---------------------------------------------------------------------------
// Pixel-protocol emitters: structural byte-shape validation (not full
// decoders — framing, chunking and range rules the protocols mandate).
// ---------------------------------------------------------------------------

/// Split a kitty emission into APC frames; validate framing + chunking.
/// Returns (control-data, payload) per frame.
fn validate_kitty_frames(bytes: &[u8]) -> Vec<(String, String)> {
    let mut frames = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        assert!(rest.starts_with(b"\x1b_G"), "frame must start ESC _ G");
        let end = rest
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .expect("frame must end with ST");
        let body = &rest[3..end];
        let semi = body.iter().position(|&b| b == b';');
        let (ctrl, payload) = match semi {
            Some(i) => (&body[..i], &body[i + 1..]),
            None => (body, &b""[..]),
        };
        let ctrl = String::from_utf8(ctrl.to_vec()).expect("control data is ASCII");
        let payload = String::from_utf8(payload.to_vec()).expect("payload is base64 text");
        // Control data: comma-separated k=v, keys single letters.
        for kv in ctrl.split(',').filter(|s| !s.is_empty()) {
            let (k, v) = kv
                .split_once('=')
                .unwrap_or_else(|| panic!("bad kv {kv:?} in {ctrl}"));
            assert!(!k.is_empty() && !v.is_empty(), "empty kv in {ctrl}");
        }
        frames.push((ctrl, payload));
        rest = &rest[end + 2..];
    }
    assert!(!frames.is_empty(), "no APC frames found");
    // Chunking rules (kitty spec): payloads ≤ 4096; every NON-final
    // chunk multiple of 4 and flagged m=1; final chunk m=0 (or no m when
    // single-frame).
    let n = frames.len();
    for (i, (ctrl, payload)) in frames.iter().enumerate() {
        assert!(
            payload.len() <= 4096,
            "chunk {i} payload {} > 4096",
            payload.len()
        );
        let m = ctrl
            .split(',')
            .find_map(|kv| kv.strip_prefix("m="))
            .unwrap_or("0");
        if i + 1 < n {
            assert_eq!(m, "1", "non-final chunk {i} must carry m=1: {ctrl}");
            assert_eq!(payload.len() % 4, 0, "non-final chunk {i} not 4-aligned");
        } else {
            assert_eq!(m, "0", "final chunk must carry m=0 (or default): {ctrl}");
        }
    }
    // The base64 must be valid as a WHOLE (chunks split one stream).
    let joined: String = frames.iter().map(|(_, p)| p.as_str()).collect();
    abstracttui::gfx::base64::decode(&joined).expect("kitty payload must be valid base64");
    frames
}

#[test]
fn kitty_transmit_chunking_and_framing_rules() {
    use abstracttui::gfx::proto::kitty;
    // Big enough to force multiple 4096-byte chunks.
    let img = Bitmap::from_fn(96, 96, |x, y| Rgba::rgb(x as u8, y as u8, 128));
    let opts = kitty::Options {
        id: 42,
        ..kitty::Options::default()
    };
    let bytes = kitty::transmit_display(&img, &opts);
    let frames = validate_kitty_frames(&bytes);
    assert!(
        frames.len() > 1,
        "96x96 RGBA must chunk (got {} frame)",
        frames.len()
    );
    let first = &frames[0].0;
    assert!(
        first.contains("i=42"),
        "id must ride the first chunk: {first}"
    );
    assert!(
        first.contains("a=T"),
        "transmit+display action expected: {first}"
    );
    // Continuation frames carry ONLY m (and optionally q) per spec.
    for (ctrl, _) in &frames[1..] {
        for kv in ctrl.split(',').filter(|s| !s.is_empty()) {
            assert!(
                kv.starts_with("m=") || kv.starts_with("q="),
                "continuation chunk carries foreign key {kv:?} (spec: m/q only)"
            );
        }
    }
    // Deletes reference the same id.
    let del = kitty::delete_by_id(42, true);
    assert!(String::from_utf8_lossy(&del).contains("i=42"));
}

/// Sixel structural validator: DCS framing, raster attributes, register
/// definitions in range, data bytes legal, RLE well-formed.
fn validate_sixel(bytes: &[u8], max_register: u16) {
    assert!(bytes.starts_with(b"\x1bP"), "sixel starts ESC P");
    assert!(bytes.ends_with(b"\x1b\\"), "sixel ends ST");
    let body_start = bytes.iter().position(|&b| b == b'q').expect("DCS ... q") + 1;
    let body = &bytes[body_start..bytes.len() - 2];
    let mut i = 0;
    let mut defined: Vec<u16> = Vec::new();
    while i < body.len() {
        match body[i] {
            b'"' => {
                // Raster attributes: 4 numeric params.
                i += 1;
                let mut params = 1;
                while i < body.len() && (body[i].is_ascii_digit() || body[i] == b';') {
                    if body[i] == b';' {
                        params += 1;
                    }
                    i += 1;
                }
                assert_eq!(params, 4, "raster attributes must carry Pan;Pad;Ph;Pv");
            }
            b'#' => {
                i += 1;
                let mut nums: Vec<u32> = vec![0];
                let mut any = false;
                while i < body.len() && (body[i].is_ascii_digit() || body[i] == b';') {
                    if body[i] == b';' {
                        nums.push(0);
                    } else {
                        any = true;
                        let last = nums.last_mut().expect("nonempty");
                        *last = *last * 10 + (body[i] - b'0') as u32;
                    }
                    i += 1;
                }
                assert!(any, "# with no register number");
                let reg = nums[0] as u16;
                assert!(
                    reg < max_register,
                    "register {reg} out of budget {max_register}"
                );
                if nums.len() > 1 {
                    // Definition: #Pc;Pu;Px;Py;Pz with Pu=2 (RGB) and
                    // channels 0..=100.
                    assert_eq!(nums.len(), 5, "malformed color def {nums:?}");
                    assert_eq!(nums[1], 2, "RGB colorspace expected");
                    for c in &nums[2..] {
                        assert!(*c <= 100, "sixel channel {c} > 100 in {nums:?}");
                    }
                    defined.push(reg);
                }
            }
            b'!' => {
                i += 1;
                let mut digits = 0;
                while i < body.len() && body[i].is_ascii_digit() {
                    digits += 1;
                    i += 1;
                }
                assert!(digits > 0, "! without a count");
                assert!(
                    i < body.len() && (0x3f..=0x7e).contains(&body[i]),
                    "! count not followed by a data byte"
                );
                i += 1;
            }
            b'$' | b'-' => i += 1,
            b if (0x3f..=0x7e).contains(&b) => i += 1,
            other => panic!("illegal sixel body byte 0x{other:02x} at {i}"),
        }
    }
    assert!(!defined.is_empty(), "no color registers defined");
}

#[test]
fn sixel_structure_registers_and_transparency() {
    use abstracttui::gfx::proto::sixel;
    let img = Bitmap::from_fn(37, 23, |x, y| {
        if x < 5 {
            Rgba::TRANSPARENT // transparency holes (P2=1)
        } else {
            Rgba::rgb((x * 6) as u8, (y * 11) as u8, ((x + y) * 5) as u8)
        }
    });
    let opts = sixel::Options::default();
    let bytes = sixel::encode(&img, &opts);
    // P2=1 (transparency) must be declared in the DCS params.
    let header = &bytes[..bytes.iter().position(|&b| b == b'q').unwrap()];
    let header_s = String::from_utf8_lossy(header);
    assert!(
        header_s.contains(";1;"),
        "P2=1 transparency expected in {header_s:?}"
    );
    validate_sixel(&bytes, opts.max_registers);
    // Register budget respected under clamping too.
    let tiny = sixel::Options {
        max_registers: 4,
        ..sixel::Options::default()
    };
    let bytes = sixel::encode(&img, &tiny);
    validate_sixel(&bytes, 4);
    // Empty bitmap: empty emission, never a malformed frame.
    let empty = sixel::encode(&Bitmap::new(0, 0, Rgba::TRANSPARENT), &opts);
    assert!(empty.is_empty());
}

/// Two emissions with the same register base BOTH define register 0 —
/// the documented v1 clobber (single-live-image limit). Pinned so the
/// day the pipeline hosts two live sixel images, this fails and forces
/// the partitioning decision (RT1-11).
#[test]
fn sixel_two_image_register_clobber_is_the_documented_v1_limit() {
    use abstracttui::gfx::proto::sixel;
    let a = sixel::encode(
        &Bitmap::from_fn(8, 8, |_, _| Rgba::rgb(200, 0, 0)),
        &sixel::Options::default(),
    );
    let b = sixel::encode(
        &Bitmap::from_fn(8, 8, |_, _| Rgba::rgb(0, 0, 200)),
        &sixel::Options::default(),
    );
    let defines_reg0 = |bytes: &[u8]| {
        let s = String::from_utf8_lossy(bytes);
        s.contains("#0;2;")
    };
    assert!(
        defines_reg0(&a) && defines_reg0(&b),
        "both emissions redefine register 0: the documented single-image limit"
    );
}

#[test]
fn iterm2_inline_png_shape() {
    use abstracttui::gfx::proto::iterm2;
    let img = Bitmap::from_fn(6, 4, |x, y| Rgba::rgb(x as u8 * 40, y as u8 * 60, 9));
    let bytes = iterm2::inline_png(&img, &iterm2::Options::default());
    let s = String::from_utf8(bytes).expect("OSC 1337 is ASCII + base64");
    assert!(s.starts_with("\x1b]1337;File="), "OSC 1337 File= framing");
    assert!(
        s.ends_with('\u{7}') || s.ends_with("\x1b\\"),
        "BEL or ST terminator"
    );
    assert!(
        s.contains("inline=1"),
        "inline=1 or the terminal DOWNLOADS the file"
    );
    // Payload after ':' is base64 of a real PNG.
    let colon = s.find(':').expect("args : payload");
    let payload = s[colon + 1..]
        .trim_end_matches('\u{7}')
        .trim_end_matches("\x1b\\");
    let decoded = abstracttui::gfx::base64::decode(payload).expect("valid base64");
    assert!(
        decoded.starts_with(b"\x89PNG\r\n\x1a\n"),
        "payload must be a PNG file"
    );
    // size= (when present) must match the decoded byte count.
    if let Some(pos) = s.find("size=") {
        let tail = &s[pos + 5..];
        let n: usize = tail
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .expect("size= numeric");
        assert_eq!(n, decoded.len(), "size= must be the payload byte count");
    }
}
