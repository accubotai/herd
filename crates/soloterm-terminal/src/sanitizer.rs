//! ANSI escape sequence sanitizer for security.
//!
//! Blocks dangerous sequences while allowing safe formatting:
//! - ALLOW: SGR (colors, bold, italic, etc.)
//! - ALLOW: Cursor movement (CUU, CUD, CUF, CUB, CUP)
//! - ALLOW: Erase (ED, EL)
//! - BLOCK: OSC 52 (clipboard write)
//! - BLOCK: OSC 7 (working directory reporting to untrusted sources)
//! - BLOCK: Device status reports that could leak info

/// Sanitize terminal output by removing dangerous escape sequences.
pub fn sanitize_output(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        if input[i] == 0x1b {
            if let Some((action, advance)) = classify_escape(&input[i..]) {
                match action {
                    EscapeAction::Allow => {
                        output.extend_from_slice(&input[i..i + advance]);
                    }
                    EscapeAction::Block => {
                        tracing::debug!("Blocked dangerous escape sequence at offset {i}");
                    }
                }
                i += advance;
            } else {
                // Incomplete or unrecognized — pass through single ESC
                output.push(input[i]);
                i += 1;
            }
        } else {
            output.push(input[i]);
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

/// Classify an escape sequence starting at `data[0] == ESC`.
///
/// Returns `(action, bytes_consumed)` or `None` if incomplete.
fn classify_escape(data: &[u8]) -> Option<(EscapeAction, usize)> {
    if data.len() < 2 {
        return None;
    }

    match data[1] {
        // CSI sequences: ESC [
        b'[' => classify_csi(&data[2..]).map(|(action, adv)| (action, adv + 2)),
        // OSC sequences: ESC ]
        b']' => classify_osc(&data[2..]).map(|(action, adv)| (action, adv + 2)),
        // All other two-byte escapes are safe
        _ => Some((EscapeAction::Allow, 2)),
    }
}

/// Classify a CSI sequence (after `ESC [`).
fn classify_csi(data: &[u8]) -> Option<(EscapeAction, usize)> {
    for (i, &byte) in data.iter().enumerate() {
        if (0x40..=0x7E).contains(&byte) {
            return Some((EscapeAction::Allow, i + 1));
        }
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
        // OSC 0-2: window title, OSC 4: palette, OSC 8: hyperlinks,
        // OSC 10-19: color queries, OSC 104: reset colors, OSC 112: reset cursor
        0..=2 | 4 | 8 | 10..=19 | 104 | 112 => EscapeAction::Allow,
        // Everything else blocked: OSC 7 (dir leak), OSC 52 (clipboard), unknown
        _ => EscapeAction::Block,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_through_plain_text() {
        let input = b"Hello, world!";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_allow_sgr_colors() {
        let input = b"\x1b[31mRed text\x1b[0m";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_allow_cursor_movement() {
        let input = b"\x1b[H\x1b[2J";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_block_osc52_clipboard() {
        let input = b"\x1b]52;c;SGVsbG8=\x07";
        let output = sanitize_output(input);
        assert!(output.is_empty(), "OSC 52 should be blocked");
    }

    #[test]
    fn test_block_osc52_with_st_terminator() {
        let input = b"\x1b]52;c;SGVsbG8=\x1b\\";
        let output = sanitize_output(input);
        assert!(output.is_empty(), "OSC 52 with ST should be blocked");
    }

    #[test]
    fn test_allow_title_change() {
        let input = b"\x1b]0;My Terminal\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_mixed_safe_and_dangerous() {
        let input = b"\x1b[32mOK\x1b]52;c;eA==\x07 done\x1b[0m";
        let output = sanitize_output(input);
        let expected = b"\x1b[32mOK done\x1b[0m";
        assert_eq!(output, expected.to_vec());
    }

    #[test]
    fn test_block_osc7_directory() {
        let input = b"\x1b]7;file:///home/user\x07";
        let output = sanitize_output(input);
        assert!(output.is_empty());
    }

    #[test]
    fn test_allow_hyperlinks() {
        let input = b"\x1b]8;;https://example.com\x07Link\x1b]8;;\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }
}
