//! Captured stream decoding and bounded incremental log storage.
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufReader, Read};
use std::sync::Mutex;

/// Log buffer bounds: oldest lines are dropped once either limit is crossed.
const LOG_MAX_LINES: usize = 1000;
const LOG_MAX_BYTES: usize = 512 * 1024;
/// A single captured line is truncated; the cap keeps one pathological line
/// from dominating the buffer.
const LOG_LINE_MAX: usize = 4000;
/// One captured output line. `seq` is monotonic per process so a reader can
/// poll incrementally.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeLogLine {
    pub seq: u64,
    pub timestamp: String,
    pub stream: &'static str,
    pub text: String,
}

#[derive(Debug, Default)]
pub(super) struct LogBuffer {
    lines: VecDeque<RuntimeLogLine>,
    next_seq: u64,
    bytes: usize,
    /// Set once at least one line has been dropped for exceeding the bounds.
    dropped: bool,
}

impl LogBuffer {
    pub(super) fn push(&mut self, stream: &'static str, timestamp: String, mut text: String) {
        if text.len() > LOG_LINE_MAX {
            let mut end = LOG_LINE_MAX;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        self.next_seq += 1;
        let line = RuntimeLogLine {
            seq: self.next_seq,
            timestamp,
            stream,
            text,
        };
        self.bytes += line.text.len();
        self.lines.push_back(line);
        while self.lines.len() > LOG_MAX_LINES || self.bytes > LOG_MAX_BYTES {
            match self.lines.pop_front() {
                Some(removed) => {
                    self.bytes -= removed.text.len();
                    self.dropped = true;
                }
                None => break,
            }
        }
    }

    #[cfg(test)]
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn retain_within(&mut self, budget: usize) -> usize {
        if self.bytes > budget {
            self.lines.clear();
            self.bytes = 0;
            self.dropped = true;
        }
        self.bytes
    }

    pub(super) fn snapshot(&self, since: u64) -> (Vec<RuntimeLogLine>, bool) {
        (self.since(since), self.dropped)
    }

    /// Lines with `seq > since`, so incremental polling never re-reads.
    fn since(&self, since: u64) -> Vec<RuntimeLogLine> {
        self.lines
            .iter()
            .filter(|line| line.seq > since)
            .cloned()
            .collect()
    }
}

/// Reads one piped stream to EOF, feeding each line into the shared buffer.
/// Raw chunks are split on newlines here instead of `lines()` so partial
/// writes (progress bars, colored output) still surface without waiting for
/// a line ending that may never come.
pub(super) fn pump_stream<T: Read + Send + 'static>(
    stream: T,
    name: &'static str,
    logs: &Mutex<LogBuffer>,
) {
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let push = |raw: &[u8]| {
        let text = String::from_utf8_lossy(raw);
        let text = text.trim_end_matches(['\n', '\r']).to_string();
        if let Ok(mut buffer) = logs.lock() {
            buffer.push(name, chrono::Local::now().to_rfc3339(), text);
        }
    };
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    push(&line);
                }
                // An unterminated overlong run is emitted truncated so the
                // buffer bound still holds.
                if buf.len() > LOG_LINE_MAX * 2 {
                    let drained = std::mem::take(&mut buf);
                    push(&drained);
                }
            }
        }
    }
    if !buf.is_empty() {
        push(&buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_buffer_drops_oldest_and_flags_the_loss() {
        let mut buffer = LogBuffer::default();
        for i in 0..(LOG_MAX_LINES + 50) {
            buffer.push("stdout", "t".into(), format!("line {i}"));
        }
        assert_eq!(buffer.lines.len(), LOG_MAX_LINES);
        assert!(buffer.dropped);
        assert_eq!(buffer.lines.front().unwrap().text, "line 50");

        // Incremental reads only return newer lines.
        let since = buffer.next_seq - 10;
        assert_eq!(buffer.since(since).len(), 10);
    }

    #[test]
    fn log_buffer_truncates_a_pathological_line() {
        let mut buffer = LogBuffer::default();
        buffer.push("stderr", "t".into(), "x".repeat(LOG_LINE_MAX * 4));
        assert_eq!(buffer.lines[0].text.len(), LOG_LINE_MAX);
    }
}
