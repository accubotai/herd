//! ANSI escape sequence sanitizer for security.
//!
//! Blocks dangerous sequences while allowing safe formatting:
//! - ALLOW: SGR (colors, bold, italic, etc.)
//! - ALLOW: Cursor movement (CUU, CUD, CUF, CUB, CUP)
//! - ALLOW: Erase (ED, EL)
//! - BLOCK: OSC 52 (clipboard write)
//! - BLOCK: OSC 7 (working directory leak)
//! - BLOCK: DCS, PM, APC (arbitrary payload sequences)
//! - BLOCK: 8-bit C1 control codes (0x90, 0x9B, 0x9D, 0x9E, 0x9F)
//! - BLOCK: CSI DSR/window manipulation sequences

/// Sanitize terminal output by removing dangerous escape sequences.
///
/// Strips lone ESC bytes that don't form a recognized safe sequence,
/// and blocks all 8-bit C1 control codes.
pub fn sanitize_output(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        let byte = input[i];

        // Block 8-bit C1 control codes (bypass for ESC-based sequences)
        if is_c1_code(byte) {
            let advance = consume_c1_sequence(&input[i..]);
            tracing::debug!("Blocked 8-bit C1 code 0x{byte:02x} at offset {i}");
            i += advance;
            continue;
        }

        if byte == 0x1b {
            if let Some((action, advance)) = classify_escape(&input[i..]) {
                match action {
                    EscapeAction::Allow => {
                        output.extend_from_slice(&input[i..i + advance]);
                    }
                    EscapeAction::Block => {
                        tracing::debug!("Blocked escape sequence at offset {i}");
                    }
                }
                i += advance;
            } else {
                // Incomplete escape at end of input — strip it (don't pass through)
                tracing::debug!("Stripped incomplete escape at offset {i}");
                i += 1;
            }
        } else {
            output.push(byte);
            i += 1;
        }
    }

    output
}

#[derive(Debug, PartialEq, Eq)]
enum EscapeAction {
    Allow,
    Block,
}

/// Check if a byte is an 8-bit C1 control code.
fn is_c1_code(byte: u8) -> bool {
    matches!(byte, 0x90 | 0x9B | 0x9D | 0x9E | 0x9F)
}

/// Consume an 8-bit C1 sequence. These use ST (0x9C) as terminator.
/// Returns number of bytes to skip.
fn consume_c1_sequence(data: &[u8]) -> usize {
    if data[0] == 0x9B {
        // 0x9B = CSI (8-bit) — find final byte
        for (i, &byte) in data.iter().enumerate().skip(1) {
            if (0x40..=0x7E).contains(&byte) {
                return i + 1;
            }
        }
        1
    } else {
        // 0x90 (DCS), 0x9D (OSC), 0x9E (PM), 0x9F (APC) — find ST (0x9C)
        for (i, &byte) in data.iter().enumerate().skip(1) {
            if byte == 0x9C {
                return i + 1;
            }
        }
        1
    }
}

/// Classify a 7-bit escape sequence starting at `data[0] == ESC`.
fn classify_escape(data: &[u8]) -> Option<(EscapeAction, usize)> {
    if data.len() < 2 {
        return None;
    }

    match data[1] {
        // CSI: ESC [
        b'[' => classify_csi(&data[2..]).map(|(action, adv)| (action, adv + 2)),
        // OSC: ESC ]
        b']' => classify_osc(&data[2..]).map(|(action, adv)| (action, adv + 2)),
        // DCS (ESC P), PM (ESC ^), APC (ESC _) — block: consume until ST
        b'P' | b'^' | b'_' => {
            let advance = consume_until_st(&data[2..]);
            Some((EscapeAction::Block, advance + 2))
        }
        // Simple safe two-byte escapes
        b'(' | b')' | b'*' | b'+' | b'7' | b'8' | b'=' | b'>' | b'c' | b'D' | b'E' | b'H'
        | b'M' | b'N' | b'O' | b'Z' => Some((EscapeAction::Allow, 2)),
        // Unknown — block to be safe
        _ => Some((EscapeAction::Block, 2)),
    }
}

/// Safe CSI final bytes — allowlisted terminal operations.
const fn is_safe_csi_final(byte: u8) -> bool {
    matches!(
        byte,
        b'm' // SGR (colors/styles)
        | b'A' | b'B' | b'C' | b'D' | b'E' | b'F' | b'G' | b'H' // cursor movement
        | b'J' | b'K' // erase display/line
        | b'S' | b'T' // scroll up/down
        | b'L' | b'M' | b'P' | b'X' | b'@' // insert/delete lines/chars
        | b'r' // set scrolling region
        | b'h' | b'l' // set/reset mode
        | b'd' | b'f' // absolute positioning
    )
}

/// Classify a CSI sequence (after `ESC [`).
///
/// Only allows safe final bytes:
/// - `m` (SGR — colors/styles)
/// - `A`-`H` (cursor movement)
/// - `J`, `K` (erase)
/// - `r` (set scrolling region)
/// - `h`, `l` (set/reset mode)
/// - `@`, `L`, `M`, `P`, `X` (insert/delete)
/// - `S`, `T` (scroll up/down)
/// - `d`, `f`, `G` (absolute positioning)
///
/// Blocks: `n` (DSR), `t` (window manipulation), and other risky finals.
fn classify_csi(data: &[u8]) -> Option<(EscapeAction, usize)> {
    for (i, &byte) in data.iter().enumerate() {
        if (0x40..=0x7E).contains(&byte) {
            let action = if is_safe_csi_final(byte) {
                EscapeAction::Allow
            } else {
                EscapeAction::Block
            };
            return Some((action, i + 1));
        }
        // Parameter and intermediate bytes (0x20-0x3F) continue the sequence
        if !(0x20..=0x3F).contains(&byte) && i > 0 {
            break;
        }
    }
    None
}

/// Classify an OSC sequence (after `ESC ]`).
fn classify_osc(data: &[u8]) -> Option<(EscapeAction, usize)> {
    for (i, &byte) in data.iter().enumerate() {
        if byte == 0x07 {
            let action = classify_osc_content(&data[..i]);
            return Some((action, i + 1));
        }
        if byte == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
            let action = classify_osc_content(&data[..i]);
            return Some((action, i + 2));
        }
    }
    None
}

fn classify_osc_content(content: &[u8]) -> EscapeAction {
    let mut num = 0u32;
    for &byte in content {
        if byte.is_ascii_digit() {
            num = num * 10 + u32::from(byte - b'0');
        } else {
            break;
        }
    }

    match num {
        // Safe: window title, palette, hyperlinks, color queries, resets
        0..=2 | 4 | 8 | 10..=19 | 104 | 112 => EscapeAction::Allow,
        // Everything else: block (OSC 7, 52, unknown)
        _ => EscapeAction::Block,
    }
}

/// Consume bytes until String Terminator (ST = ESC \ or end of data).
fn consume_until_st(data: &[u8]) -> usize {
    for (i, &byte) in data.iter().enumerate() {
        // ST as ESC backslash
        if byte == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
            return i + 2;
        }
        // ST as 8-bit code
        if byte == 0x9C {
            return i + 1;
        }
    }
    data.len() // Consume all remaining if no ST found
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Basic pass-through ──

    #[test]
    fn test_pass_through_plain_text() {
        let input = b"Hello, world!";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── SGR / Colors ──

    #[test]
    fn test_allow_sgr_colors() {
        let input = b"\x1b[31mRed text\x1b[0m";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── Cursor movement ──

    #[test]
    fn test_allow_cursor_movement() {
        let input = b"\x1b[H\x1b[2J";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── OSC 52 (clipboard) ──

    #[test]
    fn test_block_osc52_clipboard() {
        let input = b"\x1b]52;c;SGVsbG8=\x07";
        assert!(
            sanitize_output(input).is_empty(),
            "OSC 52 should be blocked"
        );
    }

    #[test]
    fn test_block_osc52_with_st_terminator() {
        let input = b"\x1b]52;c;SGVsbG8=\x1b\\";
        assert!(
            sanitize_output(input).is_empty(),
            "OSC 52 with ST should be blocked"
        );
    }

    // ── OSC 0 (title) ──

    #[test]
    fn test_allow_title_change() {
        let input = b"\x1b]0;My Terminal\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── Mixed safe + dangerous ──

    #[test]
    fn test_mixed_safe_and_dangerous() {
        let input = b"\x1b[32mOK\x1b]52;c;eA==\x07 done\x1b[0m";
        let expected = b"\x1b[32mOK done\x1b[0m";
        assert_eq!(sanitize_output(input), expected.to_vec());
    }

    // ── OSC 7 (directory leak) ──

    #[test]
    fn test_block_osc7_directory() {
        let input = b"\x1b]7;file:///home/user\x07";
        assert!(sanitize_output(input).is_empty());
    }

    // ── OSC 8 (hyperlinks) ──

    #[test]
    fn test_allow_hyperlinks() {
        let input = b"\x1b]8;;https://example.com\x07Link\x1b]8;;\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── 8-bit C1 codes ──

    #[test]
    fn test_block_8bit_osc_clipboard() {
        // 0x9D = 8-bit OSC, followed by clipboard payload, 0x9C = ST
        let input = b"\x9d52;c;SGVsbG8=\x9c";
        assert!(
            sanitize_output(input).is_empty(),
            "8-bit OSC 52 should be blocked"
        );
    }

    #[test]
    fn test_block_8bit_csi() {
        // 0x9B = 8-bit CSI
        let input = b"\x9b31m";
        assert!(
            sanitize_output(input).is_empty(),
            "8-bit CSI should be blocked"
        );
    }

    // ── DCS / PM / APC ──

    #[test]
    fn test_block_dcs_sequence() {
        // ESC P ... ESC \ (DCS with ST terminator)
        let input = b"\x1bPsome payload\x1b\\after";
        let output = sanitize_output(input);
        assert_eq!(output, b"after");
    }

    #[test]
    fn test_block_apc_sequence() {
        // ESC _ ... ESC \ (APC with ST terminator)
        let input = b"\x1b_kitty graphics\x1b\\visible";
        let output = sanitize_output(input);
        assert_eq!(output, b"visible");
    }

    #[test]
    fn test_block_pm_sequence() {
        // ESC ^ ... ESC \ (PM with ST terminator)
        let input = b"\x1b^private\x1b\\ok";
        let output = sanitize_output(input);
        assert_eq!(output, b"ok");
    }

    // ── CSI allowlist ──

    #[test]
    fn test_block_csi_dsr() {
        // CSI 6n = Device Status Report — should be blocked
        let input = b"\x1b[6n";
        assert!(
            sanitize_output(input).is_empty(),
            "CSI DSR should be blocked"
        );
    }

    #[test]
    fn test_block_csi_window_manipulation() {
        // CSI 14t = report window size — should be blocked
        let input = b"\x1b[14t";
        assert!(
            sanitize_output(input).is_empty(),
            "CSI window manipulation should be blocked"
        );
    }

    #[test]
    fn test_allow_csi_erase_line() {
        // CSI 2K = erase entire line
        let input = b"\x1b[2K";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_allow_csi_scroll() {
        // CSI 3S = scroll up 3 lines
        let input = b"\x1b[3S";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    // ── Incomplete sequences ──

    #[test]
    fn test_strip_lone_esc_at_end() {
        let input = b"hello\x1b";
        let output = sanitize_output(input);
        assert_eq!(output, b"hello", "Lone ESC at end should be stripped");
    }

    // ── Unknown two-byte escapes ──

    #[test]
    fn test_block_unknown_two_byte_escape() {
        // ESC # is not in our safe list — both ESC and # are consumed/blocked
        let input = b"\x1b#visible";
        let output = sanitize_output(input);
        assert_eq!(output, b"visible");
    }
}
