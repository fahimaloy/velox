use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum VNode {
    Element {
        tag: String,
        props: Props,
        children: Vec<VNode>,
    },
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Props {
    pub attrs: HashMap<String, String>,
}

impl Props {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
        }
    }
    pub fn set(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.attrs.insert(k.into(), v.into());
        self
    }
}

// Allow concise props creation
impl From<()> for Props {
    fn from(_: ()) -> Self {
        Props::default()
    }
}
impl From<Vec<(&str, &str)>> for Props {
    fn from(v: Vec<(&str, &str)>) -> Self {
        let mut p = Props::new();
        for (k, v) in v {
            p.attrs.insert(k.to_string(), v.to_string());
        }
        p
    }
}

pub fn h(tag: impl Into<String>, props: impl Into<Props>, children: Vec<VNode>) -> VNode {
    VNode::Element {
        tag: tag.into(),
        props: props.into(),
        children,
    }
}
pub fn text(t: impl Into<String>) -> VNode {
    VNode::Text(t.into())
}

pub mod diff;

pub mod layout;

pub mod style;

pub mod text_wrap;

pub use style::*;

/// Unified error type for all Velox operations.
///
/// This enum provides structured error handling across the Velox framework,
/// replacing ad-hoc `String` errors with typed, matchable variants.
#[derive(Debug, Clone)]
pub enum VeloxError {
    /// SFC template parsing failed
    SfcParse(String),
    /// CSS stylesheet parsing failed
    CssParse(String),
    /// Template code generation failed
    Codegen(String),
    /// Layout computation failed
    Layout(String),
    /// Rendering backend error (Skia, EGL, softbuffer)
    Render(String),
    /// Window creation or management error
    Window(String),
    /// Build/Cargo invocation failed
    Build(String),
    /// File I/O error
    Io(String),
    /// Feature not enabled (e.g., skia-native)
    FeatureNotEnabled(String),
    /// Generic internal error
    Internal(String),
}

impl std::fmt::Display for VeloxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SfcParse(msg) => write!(f, "SFC parse error: {msg}"),
            Self::CssParse(msg) => write!(f, "CSS parse error: {msg}"),
            Self::Codegen(msg) => write!(f, "Code generation error: {msg}"),
            Self::Layout(msg) => write!(f, "Layout error: {msg}"),
            Self::Render(msg) => write!(f, "Render error: {msg}"),
            Self::Window(msg) => write!(f, "Window error: {msg}"),
            Self::Build(msg) => write!(f, "Build error: {msg}"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::FeatureNotEnabled(msg) => write!(f, "Feature not enabled: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for VeloxError {}

impl From<std::io::Error> for VeloxError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<String> for VeloxError {
    fn from(msg: String) -> Self {
        Self::Internal(msg)
    }
}

impl From<&str> for VeloxError {
    fn from(msg: &str) -> Self {
        Self::Internal(msg.to_string())
    }
}

/// Convenience result type for Velox operations.
pub type Result<T> = std::result::Result<T, VeloxError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tree() {
        let node = h(
            "div",
            vec![("class", "app")],
            vec![text("hello"), h("span", (), vec![text("world")])],
        );
        if let VNode::Element {
            tag,
            props,
            children,
        } = node
        {
            assert_eq!(tag, "div");
            assert_eq!(props.attrs.get("class").unwrap(), "app");
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected element");
        }
    }
}
