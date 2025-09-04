#[derive(Debug, Clone, PartialEq)]
pub enum AttrKind {
    Static, // class="app"
    Bind,   // :value="count"
    On,     // @click="increment"
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateAttr {
    pub name: String,
    pub value: Option<String>,
    pub kind: AttrKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element {
        tag: String,
        attrs: Vec<TemplateAttr>,
        children: Vec<Node>,
        self_closing: bool,
    },
    Text(String),
    Interpolation(String), // {{ expr }}
}
