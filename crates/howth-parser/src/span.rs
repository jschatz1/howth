//! Source location tracking.
//!
//! Every AST node has a `Span` indicating its position in the source code.
//! This is essential for error messages and source map generation.

/// A span in the source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Byte offset of the start.
    pub start: u32,
    /// Byte offset of the end (exclusive).
    pub end: u32,
}

impl Span {
    /// Create a new span.
    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Create an empty span at a position.
    #[inline]
    pub const fn empty(pos: u32) -> Self {
        Self { start: pos, end: pos }
    }

    /// Length of the span in bytes.
    #[inline]
    pub const fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Check if the span is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into one that covers both.
    #[inline]
    pub const fn merge(self, other: Span) -> Span {
        Span {
            start: if self.start < other.start { self.start } else { other.start },
            end: if self.end > other.end { self.end } else { other.end },
        }
    }

    /// Check if this span contains a byte offset.
    #[inline]
    pub const fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// A value with an associated span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Create a new spanned value.
    #[inline]
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Map the inner value.
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }
}

/// Convert line/column to byte offset and vice versa.
#[derive(Debug)]
pub struct LineIndex {
    /// Byte offsets of the start of each line.
    line_starts: Vec<u32>,
}

impl LineIndex {
    /// Build a line index from source code.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self { line_starts }
    }

    /// Convert a byte offset to line and column (both 0-indexed).
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = self.line_starts
            .binary_search(&offset)
            .unwrap_or_else(|i| i.saturating_sub(1));
        let col = offset - self.line_starts[line];
        (line as u32, col)
    }

    /// Convert line and column (both 0-indexed) to byte offset.
    pub fn offset(&self, line: u32, col: u32) -> u32 {
        self.line_starts.get(line as usize).copied().unwrap_or(0) + col
    }

    /// Get the total number of lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_merge() {
        let a = Span::new(5, 10);
        let b = Span::new(8, 15);
        assert_eq!(a.merge(b), Span::new(5, 15));
    }

    #[test]
    fn test_line_index() {
        let source = "line1\nline2\nline3";
        let index = LineIndex::new(source);

        assert_eq!(index.line_col(0), (0, 0));  // 'l' of line1
        assert_eq!(index.line_col(5), (0, 5));  // '\n' after line1
        assert_eq!(index.line_col(6), (1, 0));  // 'l' of line2
        assert_eq!(index.line_col(12), (2, 0)); // 'l' of line3
    }
}
