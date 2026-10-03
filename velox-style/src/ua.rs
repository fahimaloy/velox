use std::sync::OnceLock;

use crate::Stylesheet;

static UA_SRC: &str = include_str!("ua.css");

static UA: OnceLock<Stylesheet> = OnceLock::new();

pub fn ua_sheet() -> &'static Stylesheet {
    UA.get_or_init(|| Stylesheet::parse(UA_SRC))
}
