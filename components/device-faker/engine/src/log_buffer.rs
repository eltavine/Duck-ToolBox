//! Bounded pre-specialization diagnostics for the companion's framed transport.
use std::collections::VecDeque;

const MAX_BYTES: usize = 24 * 1024;
const MAX_LINES: usize = 256;
const MAX_LINE_BYTES: usize = 2048;

pub(crate) struct Buffer {
    lines: VecDeque<String>,
    bytes: usize,
    dropped: usize,
}
impl Buffer {
    pub(crate) const fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            bytes: 0,
            dropped: 0,
        }
    }
    pub(crate) fn push(&mut self, mut line: String) {
        if line.len() > MAX_LINE_BYTES {
            let suffix = "… [truncated]";
            let mut end = MAX_LINE_BYTES - suffix.len();
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            line.push_str(suffix);
        }
        while self.bytes + line.len() > MAX_BYTES || self.lines.len() >= MAX_LINES {
            if let Some(old) = self.lines.pop_front() {
                self.bytes -= old.len();
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        self.bytes += line.len();
        self.lines.push_back(line);
    }
    pub(crate) fn drain(&mut self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.lines.len() + usize::from(self.dropped > 0));
        if self.dropped > 0 {
            lines.push(format!(
                "[WARN] {} earlier diagnostic lines dropped",
                self.dropped
            ));
        }
        lines.extend(self.lines.drain(..));
        self.bytes = 0;
        self.dropped = 0;
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_preserve_recent_diagnostics_and_report_eviction() {
        let mut buffer = Buffer::new();
        for index in 0..MAX_LINES + 10 {
            buffer.push(format!("line {index}"));
        }
        let lines = buffer.drain();
        assert!(lines[0].contains("10 earlier"));
        assert_eq!(lines[1], "line 10");
        assert_eq!(lines.last().unwrap(), &format!("line {}", MAX_LINES + 9));
        assert!(buffer.drain().is_empty());
    }
    #[test]
    fn long_unicode_lines_truncate_at_a_character_boundary() {
        let mut buffer = Buffer::new();
        buffer.push("故障".repeat(4096));
        let lines = buffer.drain();
        assert!(lines[0].len() <= MAX_LINE_BYTES);
        assert!(lines[0].starts_with("故障"));
        assert!(lines[0].ends_with("[truncated]"));
    }
    #[test]
    fn maximum_json_escaping_still_fits_the_companion_frame() {
        let mut buffer = Buffer::new();
        for _ in 0..100 {
            buffer.push("\u{1}".repeat(4096));
        }
        let lines = buffer.drain();
        assert!(lines[0].contains("diagnostic lines dropped"));
        assert!(lines.iter().map(String::len).sum::<usize>() < MAX_BYTES + 128);
        let frame = serde_json::to_vec(&serde_json::json!({"WriteLog":{"lines":lines}})).unwrap();
        assert!(frame.len() < 256 * 1024);
    }
}
