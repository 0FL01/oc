//! Bounded terminal clipboard transport. OSC 52 sends a request to the
//! terminal's clipboard selection; only the terminal write can be confirmed
//! here, not acceptance by the terminal or the desktop/SSH clipboard.

use std::io::{self, IsTerminal as _};

const MAX_TEXT_BYTES: usize = oc_tui::app::MAX_SELECTION_BYTES;
const PREFIX: &[u8] = b"\x1b]52;c;";
const TERMINATOR: &[u8] = b"\x07";
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Return success only after a complete OSC 52 request was written and
/// flushed to an actual terminal. No clipboard payload or IO error details
/// are included in user-visible feedback or tracing.
pub fn copy(text: &str) -> Result<(), String> {
    let stdout = io::stdout();
    if !stdout.is_terminal() {
        return Err("clipboard unavailable: terminal output is not a TTY".into());
    }
    write_osc52(&mut stdout.lock(), text)
}

fn write_osc52(writer: &mut impl io::Write, text: &str) -> Result<(), String> {
    // Match pinned upstream's NUL removal before writing text to clipboard.
    // Validate the source as well as the filtered text: hostile callers must
    // not use a huge NUL-only allocation to bypass the byte ceiling.
    if text.len() > MAX_TEXT_BYTES {
        return Err("clipboard selection exceeds size limit".into());
    }
    let cleaned = text.replace('\0', "");
    if cleaned.is_empty() {
        return Err("clipboard selection is empty".into());
    }
    let encoded = encode_base64(cleaned.as_bytes());
    // Construct one complete escape sequence before writing so payload bytes
    // can never be interpreted as terminal controls.
    let mut request = Vec::with_capacity(PREFIX.len() + encoded.len() + TERMINATOR.len());
    request.extend_from_slice(PREFIX);
    request.extend_from_slice(&encoded);
    request.extend_from_slice(TERMINATOR);
    writer
        .write_all(&request)
        .and_then(|()| writer.flush())
        .map_err(|_| "clipboard unavailable: terminal write failed".into())
}

// A tiny encoder avoids adding a new direct dependency (and changing the
// workspace lockfile) for this single, bounded OSC 52 transport.
fn encode_base64(bytes: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(4 * bytes.len().div_ceil(3));
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize]);
        encoded.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize]);
        encoded.push(if chunk.len() >= 2 {
            ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize]
        } else {
            b'='
        });
        encoded.push(if chunk.len() == 3 {
            ALPHABET[(third & 0x3f) as usize]
        } else {
            b'='
        });
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_clipboard_selection_removes_nul_and_encodes_controls() {
        let mut output = Vec::new();
        write_osc52(&mut output, "hi\0\x1b]52;c;injected\x07☃").unwrap();
        assert!(output.starts_with(PREFIX));
        assert!(output.ends_with(TERMINATOR));
        let payload = &output[PREFIX.len()..output.len() - TERMINATOR.len()];
        assert!(
            payload
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'/' | b'='))
        );
        assert_eq!(payload, b"aGkbXTUyO2M7aW5qZWN0ZWQH4piD");
        assert_eq!(output.iter().filter(|&&byte| byte == b'\x1b').count(), 1);
        assert_eq!(output.iter().filter(|&&byte| byte == b'\x07').count(), 1);
    }

    #[test]
    fn base64_padding_for_one_two_and_three_bytes() {
        assert_eq!(encode_base64(b"f"), b"Zg==");
        assert_eq!(encode_base64(b"fo"), b"Zm8=");
        assert_eq!(encode_base64(b"foo"), b"Zm9v");
    }

    struct FailingWriter {
        fail_flush: bool,
    }

    impl io::Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(io::Error::other("secret failure content"))
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("secret failure content"))
        }
    }

    #[test]
    fn failed_write_or_flush_does_not_claim_success_or_leak_content() {
        for fail_flush in [false, true] {
            let error = write_osc52(&mut FailingWriter { fail_flush }, "private text")
                .expect_err("failed transport must not report success");
            assert_eq!(error, "clipboard unavailable: terminal write failed");
        }
    }

    #[test]
    fn empty_and_oversized_selections_never_reach_writer() {
        let mut output = Vec::new();
        assert!(write_osc52(&mut output, "\0\0").is_err());
        assert!(write_osc52(&mut output, &"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
        assert!(output.is_empty());
        write_osc52(&mut output, &"x".repeat(MAX_TEXT_BYTES)).unwrap();
        assert_eq!(
            output.len(),
            PREFIX.len() + 4 * MAX_TEXT_BYTES.div_ceil(3) + 1
        );
    }
}
