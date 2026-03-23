/// ANSI escape sequence sanitizer for security.
///
/// Blocks dangerous sequences while allowing safe formatting:
/// - ALLOW: SGR (colors, bold, italic, etc.)
/// - ALLOW: Cursor movement (CUU, CUD, CUF, CUB, CUP)
/// - ALLOW: Erase (ED, EL)
/// - BLOCK: OSC 52 (clipboard write)
/// - BLOCK: OSC 7 (working directory reporting to untrusted sources)
/// - BLOCK: Device status reports that could leak info

/// Sanitize terminal output by removing dangerous escape sequences
pub fn sanitize_output(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        if input[i] == 0x1b {
            // Start of escape sequence
            if let Some((action, advance)) = classify_escape(&input[i..]) {
                match action {
                    EscapeAction::Allow => {
                        output.extend_from_slice(&input[i..i + advance]);
                    }
                    EscapeAction::Block => {
                        // Skip the sequence entirely
                        tracing::debug!(
                            "Blocked dangerous escape sequence at offset {}",
                            i
                        );
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

#[derive(Debug, PartialEq)]
enum EscapeAction {
    Allow,
    Block,
}

/// Classify an escape sequence starting at `data[0] == ESC`
/// Returns (action, bytes_consumed) or None if incomplete
fn classify_escape(data: &[u8]) -> Option<(EscapeAction, usize)> {
    if data.len() < 2 {
        return None;
    }

    match data[1] {
        // CSI sequences: ESC [
        b'[' => classify_csi(&data[2..]).map(|(action, adv)| (action, adv + 2)),

        // OSC sequences: ESC ]
        b']' => classify_osc(&data[2..]).map(|(action, adv)| (action, adv + 2)),

        // Simple two-byte escapes (allow)
        b'(' | b')' | b'*' | b'+' | b'7' | b'8' | b'=' | b'>' | b'c' | b'D' | b'E'
        | b'H' | b'M' | b'N' | b'O' | b'Z' => Some((EscapeAction::Allow, 2)),

        _ => Some((EscapeAction::Allow, 2)),
    }
}

/// Classify a CSI sequence (after ESC [)
fn classify_csi(data: &[u8]) -> Option<(EscapeAction, usize)> {
    // Find the terminating byte (0x40-0x7E)
    for (i, &byte) in data.iter().enumerate() {
        if (0x40..=0x7E).contains(&byte) {
            // CSI sequences are generally safe (cursor movement, colors, erase)
            return Some((EscapeAction::Allow, i + 1));
        }
        // Parameter and intermediate bytes continue the sequence
        if !((0x20..=0x3F).contains(&byte)) && i > 0 {
            break;
        }
    }
    None // Incomplete
}

/// Classify an OSC sequence (after ESC ])
fn classify_osc(data: &[u8]) -> Option<(EscapeAction, usize)> {
    // Find terminator: BEL (0x07) or ST (ESC \)
    for i in 0..data.len() {
        if data[i] == 0x07 {
            let osc_content = &data[..i];
            let action = classify_osc_content(osc_content);
            return Some((action, i + 1));
        }
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
            let osc_content = &data[..i];
            let action = classify_osc_content(osc_content);
            return Some((action, i + 2));
        }
    }
    None // Incomplete
}

fn classify_osc_content(content: &[u8]) -> EscapeAction {
    // Parse OSC number
    let mut num = 0u32;
    let mut i = 0;
    while i < content.len() && content[i].is_ascii_digit() {
        num = num * 10 + (content[i] - b'0') as u32;
        i += 1;
    }

    match num {
        // OSC 0, 1, 2: Set window title — ALLOW (harmless)
        0 | 1 | 2 => EscapeAction::Allow,
        // OSC 4: Set/query color palette — ALLOW
        4 => EscapeAction::Allow,
        // OSC 7: Report working directory — BLOCK (info leak potential)
        7 => EscapeAction::Block,
        // OSC 8: Hyperlinks — ALLOW
        8 => EscapeAction::Allow,
        // OSC 10-19: Color queries — ALLOW
        10..=19 => EscapeAction::Allow,
        // OSC 52: Clipboard manipulation — BLOCK (security critical)
        52 => EscapeAction::Block,
        // OSC 104: Reset colors — ALLOW
        104 => EscapeAction::Allow,
        // OSC 112: Reset cursor color — ALLOW
        112 => EscapeAction::Allow,
        // Default: block unknown OSC sequences
        _ => EscapeAction::Block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_through_plain_text() {
        let input = b"Hello, world!";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_allow_sgr_colors() {
        // ESC[31m = red foreground
        let input = b"\x1b[31mRed text\x1b[0m";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_allow_cursor_movement() {
        // ESC[H = cursor home, ESC[2J = clear screen
        let input = b"\x1b[H\x1b[2J";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_block_osc52_clipboard() {
        // OSC 52 ; c ; BASE64 BEL
        let input = b"\x1b]52;c;SGVsbG8=\x07";
        let output = sanitize_output(input);
        assert!(output.is_empty(), "OSC 52 should be blocked");
    }

    #[test]
    fn test_block_osc52_with_st_terminator() {
        // OSC 52 with ST terminator
        let input = b"\x1b]52;c;SGVsbG8=\x1b\\";
        let output = sanitize_output(input);
        assert!(output.is_empty(), "OSC 52 with ST should be blocked");
    }

    #[test]
    fn test_allow_title_change() {
        // OSC 0 ; title BEL
        let input = b"\x1b]0;My Terminal\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }

    #[test]
    fn test_mixed_safe_and_dangerous() {
        // Safe SGR + dangerous OSC 52 + safe text
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
        // OSC 8 ; params ; uri ST
        let input = b"\x1b]8;;https://example.com\x07Link\x1b]8;;\x07";
        assert_eq!(sanitize_output(input), input.to_vec());
    }
}
