use pest::Parser;
use pest::iterators::Pair;

#[derive(pest_derive::Parser)]
#[grammar = "grammar.pest"]
struct SfcParser;

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub name: String,
    pub value: Option<String>, // boolean attrs allowed, e.g., `scoped` or `setup`
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
    pub setup: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sfc {
    pub template: Option<TemplateBlock>,
    pub script_setup: Option<ScriptBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
}

pub fn parse_sfc(source: &str) -> Result<Sfc, String> {
    let mut sfc = Sfc::default();

    // Parse the root and immediately descend into the `file` node.
    let mut pairs = SfcParser::parse(Rule::file, source).map_err(|e| e.to_string())?;
    let file = pairs.next().ok_or_else(|| "empty SFC".to_string())?;
    debug_assert!(file.as_rule() == Rule::file);

    // Walk children of `file`: they will be `block` nodes (and nothing else,
    // since WS is a silent rule in the grammar).
    for node in file.into_inner() {
        match node.as_rule() {
            Rule::block => {
                for inner in node.into_inner() {
                    consume_top_level(inner, &mut sfc);
                }
            }
            // (Defensive: in case grammar changes and blocks appear directly)
            Rule::template | Rule::script | Rule::style => {
                consume_top_level(node, &mut sfc);
            }
            _ => {}
        }
    }

    Ok(sfc)
}

fn consume_top_level(node: Pair<Rule>, sfc: &mut Sfc) {
    match node.as_rule() {
        Rule::template => {
            let (attrs, content) = parse_template(node);
            sfc.template = Some(TemplateBlock { attrs, content });
        }
        Rule::script => {
            let (attrs, content) = parse_script(node);
            let setup = has_bool_attr(&attrs, "setup");
            let sb = ScriptBlock {
                attrs,
                content,
                setup,
            };
            if setup {
                sfc.script_setup = Some(sb);
            } else {
                sfc.script = Some(sb);
            }
        }
        Rule::style => {
            let (attrs, content) = parse_style(node);
            sfc.style = Some(StyleBlock { attrs, content });
        }
        _ => {}
    }
}

fn parse_template(tpl: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in tpl.into_inner() {
        match p.as_rule() {
            Rule::template_open => {
                // attributes are direct children of *_open
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::template_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_script(scr: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in scr.into_inner() {
        match p.as_rule() {
            Rule::script_open => {
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::script_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_style(sty: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in sty.into_inner() {
        match p.as_rule() {
            Rule::style_open => {
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::style_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_attr(attr: Pair<Rule>) -> Attr {
    // attribute = ident ( "=" quoted )?
    let mut name = String::new();
    let mut value: Option<String> = None;

    for part in attr.into_inner() {
        match part.as_rule() {
            Rule::ident => name = part.as_str().to_string(),
            Rule::quoted => value = Some(strip_quotes(part.as_str())),
            _ => {}
        }
    }
    Attr { name, value }
}

fn strip_quotes(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn has_bool_attr(attrs: &[Attr], key: &str) -> bool {
    attrs.iter().any(|a| a.name == key)
}
