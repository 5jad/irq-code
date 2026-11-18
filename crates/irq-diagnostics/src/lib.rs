//! Stable diagnostic model; rendering (terminal/JSON) belongs in separate layers.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub kind: &'static str,
    pub message: String,
}
