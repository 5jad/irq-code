//! Stable diagnostics, source locations, and JSON rendering for IRQ tools.

pub const SCHEMA_VERSION: &str = "1.0.0";

/// Stable diagnostic identifiers. Never recycle a code for a different meaning.
pub mod codes {
    pub const SOURCE_TOO_LARGE: &str = "E0001";
    pub const LEX_ERROR: &str = "E0002";
    pub const TOKEN_LIMIT: &str = "E0003";
    pub const PARSE_ERROR: &str = "E0100";
    pub const EXPRESSION_ERROR: &str = "E0101";
    pub const NESTING_LIMIT: &str = "E0102";
    pub const TYPE_UNSUPPORTED: &str = "E0200";
    pub const TYPE_DUPLICATE_BINDING: &str = "E0201";
    pub const TYPE_UNREACHABLE: &str = "E0202";
    pub const TYPE_MISMATCH: &str = "E0203";
    pub const TYPE_EXPRESSION: &str = "E0204";
    pub const TYPE_UNKNOWN_NAME: &str = "E0205";
    pub const TYPE_IMMUTABLE: &str = "E0206";
    pub const TYPE_RETURN_PATH: &str = "E0207";
    pub const TYPE_DUPLICATE_FUNCTION: &str = "E0208";
    pub const TYPE_CONDITION: &str = "E0209";
    pub const CODEGEN_ERROR: &str = "E0300";
    pub const RUSTC_ERROR: &str = "E0301";
    pub const IO_ERROR: &str = "E0400";
    pub const RUSTC_UNAVAILABLE: &str = "E0401";
    pub const ARTIFACT_COMMIT_ERROR: &str = "E0402";
    pub const INVALID_USAGE: &str = "E0403";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(u32);

impl FileId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// UTF-8 byte range, half-open: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// One-based line number.
    pub line: usize,
    /// One-based Unicode-scalar column; tabs count as one column.
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub name: String,
    text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Owns source files for one compiler invocation. IDs are stable for its lifetime.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add_file(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile {
            name: name.into(),
            text,
            line_starts,
        });
        id
    }

    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.index())
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn location(&self, id: FileId, byte_offset: usize) -> Option<Location> {
        let file = self.file(id)?;
        if byte_offset > file.text.len() || !file.text.is_char_boundary(byte_offset) {
            return None;
        }
        let line_index = file
            .line_starts
            .partition_point(|start| *start <= byte_offset)
            - 1;
        let line_start = file.line_starts[line_index];
        let column = file.text[line_start..byte_offset].chars().count() + 1;
        Some(Location {
            line: line_index + 1,
            column,
        })
    }

    /// Return the byte range of a one-based line, excluding its newline bytes.
    pub fn line_span(&self, id: FileId, line: usize) -> Option<ByteSpan> {
        let file = self.file(id)?;
        if line == 0 || line > file.line_starts.len() {
            return None;
        }
        let start = file.line_starts[line - 1];
        let mut end = file
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(file.text.len());
        if end > start && file.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && file.text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        Some(ByteSpan { start, end })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub file: FileId,
    pub span: ByteSpan,
    pub message: String,
    pub primary: bool,
}

impl Label {
    pub fn primary(file: FileId, span: ByteSpan, message: impl Into<String>) -> Self {
        Self {
            file,
            span,
            message: message.into(),
            primary: true,
        }
    }

    pub fn secondary(file: FileId, span: ByteSpan, message: impl Into<String>) -> Self {
        Self {
            file,
            span,
            message: message.into(),
            primary: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub kind: &'static str,
    pub severity: Severity,
    pub message: String,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    pub fn error(code: &'static str, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            kind,
            severity: Severity::Error,
            message: message.into(),
            labels: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }
}

/// Render the stable diagnostics envelope described by `docs/diagnostic.schema.json`.
pub fn render_json(diagnostics: &[Diagnostic], sources: &SourceMap) -> String {
    let mut out = String::from("{\"schema_version\":\"");
    out.push_str(SCHEMA_VERSION);
    out.push_str("\",\"status\":\"error\",\"diagnostics\":[");
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str("{\"code\":");
        push_json_string(&mut out, diagnostic.code);
        out.push_str(",\"kind\":");
        push_json_string(&mut out, diagnostic.kind);
        out.push_str(",\"severity\":");
        push_json_string(&mut out, diagnostic.severity.as_str());
        out.push_str(",\"message\":");
        push_json_string(&mut out, &diagnostic.message);
        out.push_str(",\"labels\":[");
        for (label_index, label) in diagnostic.labels.iter().enumerate() {
            if label_index != 0 {
                out.push(',');
            }
            out.push_str("{\"primary\":");
            out.push_str(if label.primary { "true" } else { "false" });
            out.push_str(",\"file\":");
            let file = sources.file(label.file);
            push_json_string(
                &mut out,
                file.map(|f| f.name.as_str()).unwrap_or("<unknown>"),
            );
            let location = sources.location(label.file, label.span.start);
            let line = location.map(|l| l.line).unwrap_or(1);
            let column = location.map(|l| l.column).unwrap_or(1);
            out.push_str(&format!(
                ",\"line\":{line},\"column\":{column},\"start\":{},\"end\":{},\"message\":",
                label.span.start, label.span.end
            ));
            push_json_string(&mut out, &label.message);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_utf8_byte_offsets_to_unicode_columns_and_crlf_lines() {
        let mut sources = SourceMap::default();
        let file = sources.add_file("src/main.irq", "a\r\nعربي\nx");
        let arabic_start = "a\r\n".len();
        assert_eq!(
            sources.location(file, arabic_start),
            Some(Location { line: 2, column: 1 })
        );
        assert_eq!(
            sources.location(file, arabic_start + "عر".len()),
            Some(Location { line: 2, column: 3 })
        );
        assert_eq!(
            sources.line_span(file, 1),
            Some(ByteSpan { start: 0, end: 1 })
        );
        assert_eq!(
            sources.line_span(file, 2),
            Some(ByteSpan { start: 3, end: 11 })
        );
    }

    #[test]
    fn rejects_locations_inside_utf8_code_points() {
        let mut sources = SourceMap::default();
        let file = sources.add_file("x.irq", "ع");
        assert_eq!(sources.location(file, 1), None);
        assert_eq!(
            sources.location(file, 2),
            Some(Location { line: 1, column: 2 })
        );
    }

    #[test]
    fn renders_multiple_file_labels_and_escapes_json_strings() {
        let mut sources = SourceMap::default();
        let a = sources.add_file("a.irq", "fn a\n");
        let b = sources.add_file("b\".irq", "fn b\n");
        let diagnostic = Diagnostic::error(codes::PARSE_ERROR, "parse_error", "bad\n\"input\"")
            .with_label(Label::primary(a, ByteSpan { start: 0, end: 4 }, "first"))
            .with_label(Label::secondary(
                b,
                ByteSpan { start: 0, end: 4 },
                "related",
            ));
        let json = render_json(&[diagnostic], &sources);
        assert!(json.contains("\\n\\\"input\\\""));
        assert!(json.contains("b\\\".irq"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap()["diagnostics"][0]["labels"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
