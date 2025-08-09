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
