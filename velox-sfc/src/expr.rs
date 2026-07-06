//! Expression parser and transformer for Velox SFC directives.
//!
//! Handles v-if, v-else-if, :key, and interpolation expressions, converting
//! template DSL syntax (e.g., `count > 0 && is_visible`) into valid Rust code
//! that references either `resolve()` or `state.field.get()`.

// ── Token ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    BoolLit(bool),
    NumberLit(f64),
    StringLit(String),
    And,      // &&
    Or,       // ||
    Not,      // !
    Eq,       // ==
    Neq,      // !=
    Gt,       // >
    Lt,       // <
    Geq,      // >=
    Leq,      // <=
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Percent,  // %
    Question, // ?
    Colon,    // :
    LParen,   // (
    RParen,   // )
    Dot,      // .
    Comma,    // ,
    LBracket, // [
    RBracket, // ]
}

// ── Expr AST ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    BoolLit(bool),
    NumberLit(f64),
    StringLit(String),
    /// Variable or field reference (e.g., `count`, `is_visible`)
    Ident(String),
    /// Unary NOT: `!expr`
    Not(Box<Expr>),
    /// Unary minus: `-expr`
    Neg(Box<Expr>),
    /// Binary operation: `a && b`, `count > 0`
    BinOp(BinOpKind, Box<Expr>, Box<Expr>),
    /// Parenthesized: `(expr)`
    Group(Box<Expr>),
    /// Ternary: `cond ? then : else`
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Method call: `obj.method(args)` or `method(args)`
    MethodCall {
        receiver: Option<Box<Expr>>,
        method: String,
        args: Vec<Expr>,
    },
    /// Field access: `expr.field`
    FieldAccess(Box<Expr>, String),
    /// Index access: `expr[index]`
    IndexAccess(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOpKind {
    And, // &&
    Or,  // ||
    Eq,  // ==
    Neq, // !=
    Gt,  // >
    Lt,  // <
    Geq, // >=
    Leq, // <=
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Mod, // %
}

// ── Transform mode ──────────────────────────────────────────────────────────

/// Determines how identifiers are transformed in the output Rust code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformMode {
    /// Wrap identifiers in `resolve()` calls with truthy checks.
    /// Used in the `render_with()` path where values come from a closure.
    Resolve,
    /// Reference `state.field.get()` directly.
    /// Used in the `render_with_state()` path where values come from a known State struct.
    State,
}

// ── Tokenizer ───────────────────────────────────────────────────────────────

struct Tokenizer {
    input: String,
    pos: usize,
}

impl Tokenizer {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_string(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            self.pos += c.len_utf8();
            Some(c)
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        self.skip_whitespace();
        while let Some(c) = self.peek() {
            let tok = match c {
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        Token::And
                    } else {
                        return Err("unexpected single '&'".to_string());
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        Token::Or
                    } else {
                        return Err("unexpected single '|'".to_string());
                    }
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Eq
                    } else {
                        // Single '=' is assignment, not valid in template expressions
                        return Err("unexpected single '=' (use == for comparison)".to_string());
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Neq
                    } else {
                        Token::Not
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Geq
                    } else {
                        Token::Gt
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Leq
                    } else {
                        Token::Lt
                    }
                }
                '+' => {
                    self.advance();
                    Token::Plus
                }
                '-' => {
                    self.advance();
                    Token::Minus
                }
                '*' => {
                    self.advance();
                    Token::Star
                }
                '/' => {
                    self.advance();
                    Token::Slash
                }
                '%' => {
                    self.advance();
                    Token::Percent
                }
                '?' => {
                    self.advance();
                    Token::Question
                }
                ':' => {
                    self.advance();
                    Token::Colon
                }
                '(' => {
                    self.advance();
                    Token::LParen
                }
                ')' => {
                    self.advance();
                    Token::RParen
                }
                '.' => {
                    self.advance();
                    Token::Dot
                }
                ',' => {
                    self.advance();
                    Token::Comma
                }
                '[' => {
                    self.advance();
                    Token::LBracket
                }
                ']' => {
                    self.advance();
                    Token::RBracket
                }
                '"' | '\'' => {
                    let quote = c;
                    self.advance(); // consume opening quote
                    let mut s = String::new();
                    while let Some(nc) = self.peek() {
                        if nc == quote {
                            self.advance();
                            break;
                        }
                        if nc == '\\' {
                            self.advance();
                            if let Some(escaped) = self.advance() {
                                s.push(escaped);
                            }
                        } else {
                            s.push(nc);
                            self.advance();
                        }
                    }
                    Token::StringLit(s)
                }
                c if c.is_ascii_digit() => {
                    let mut num_str = String::new();
                    num_str.push(c);
                    self.advance();
                    while let Some(nc) = self.peek() {
                        if nc.is_ascii_digit() || nc == '.' {
                            num_str.push(nc);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let val: f64 = num_str
                        .parse()
                        .map_err(|e| format!("invalid number '{}': {}", num_str, e))?;
                    Token::NumberLit(val)
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    ident.push(c);
                    self.advance();
                    while let Some(nc) = self.peek() {
                        if nc.is_ascii_alphanumeric() || nc == '_' {
                            ident.push(nc);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if ident == "true" {
                        Token::BoolLit(true)
                    } else if ident == "false" {
                        Token::BoolLit(false)
                    } else {
                        Token::Ident(ident)
                    }
                }
                other => {
                    return Err(format!("unexpected character '{}' in expression", other));
                }
            };
            tokens.push(tok);
            self.skip_whitespace();
        }
        Ok(tokens)
    }
}

// ── Parser ──────────────────────────────────────────────────────────────────

/// Precedence levels for binary operators (lower = evaluated later).
const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_EQ: u8 = 3; // ==, !=
const PREC_CMP: u8 = 4; // <, >, <=, >=
const PREC_ADD: u8 = 5; // +, -
const PREC_MUL: u8 = 6; // *, /, %

fn bin_op_precedence(kind: BinOpKind) -> u8 {
    match kind {
        BinOpKind::Or => PREC_OR,
        BinOpKind::And => PREC_AND,
        BinOpKind::Eq => PREC_EQ,
        BinOpKind::Neq => PREC_EQ,
        BinOpKind::Gt => PREC_CMP,
        BinOpKind::Lt => PREC_CMP,
        BinOpKind::Geq => PREC_CMP,
        BinOpKind::Leq => PREC_CMP,
        BinOpKind::Add => PREC_ADD,
        BinOpKind::Sub => PREC_ADD,
        BinOpKind::Mul => PREC_MUL,
        BinOpKind::Div => PREC_MUL,
        BinOpKind::Mod => PREC_MUL,
    }
}

fn token_to_bin_op(tok: &Token) -> Option<BinOpKind> {
    match tok {
        Token::And => Some(BinOpKind::And),
        Token::Or => Some(BinOpKind::Or),
        Token::Eq => Some(BinOpKind::Eq),
        Token::Neq => Some(BinOpKind::Neq),
        Token::Gt => Some(BinOpKind::Gt),
        Token::Lt => Some(BinOpKind::Lt),
        Token::Geq => Some(BinOpKind::Geq),
        Token::Leq => Some(BinOpKind::Leq),
        Token::Plus => Some(BinOpKind::Add),
        Token::Minus => Some(BinOpKind::Sub),
        Token::Star => Some(BinOpKind::Mul),
        Token::Slash => Some(BinOpKind::Div),
        Token::Percent => Some(BinOpKind::Mod),
        _ => None,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.advance();
        match tok {
            Some(t) if t == *expected => Ok(()),
            Some(t) => Err(format!("expected {:?}, got {:?}", expected, t)),
            None => Err(format!("expected {:?}, got end of expression", expected)),
        }
    }

    /// Parse a full expression.
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    /// Parse ternary: `cond ? then : else`
    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let expr = self.parse_bin_op(PREC_OR)?;
        if self.peek() == Some(&Token::Question) {
            self.advance(); // consume ?
            let then_expr = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let else_expr = self.parse_expr()?;
            Ok(Expr::Ternary(
                Box::new(expr),
                Box::new(then_expr),
                Box::new(else_expr),
            ))
        } else {
            Ok(expr)
        }
    }

    /// Parse binary operators with precedence climbing.
    fn parse_bin_op(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;

        while let Some(tok) = self.peek() {
            let op_kind = token_to_bin_op(tok);
            if let Some(kind) = op_kind {
                let prec = bin_op_precedence(kind);
                if prec < min_prec {
                    break;
                }
                self.advance(); // consume operator
                let right = self.parse_bin_op(prec + 1)?;
                left = Expr::BinOp(kind, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse unary operators: `!expr`, `-expr`
    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Not) => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Not(Box::new(inner)))
            }
            Some(Token::Minus) => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Neg(Box::new(inner)))
            }
            _ => self.parse_postfix(),
        }
    }

    /// Parse postfix: method calls `.method(args)`, field access `.field`,
    /// and indexing `[expr]`.
    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.advance(); // consume .
                    // Next token must be an Ident (method name or field name)
                    let name_tok = self.advance();
                    let name = match name_tok {
                        Some(Token::Ident(s)) => s,
                        Some(t) => {
                            return Err(format!("expected identifier after '.', got {:?}", t));
                        }
                        None => {
                            return Err(
                                "expected identifier after '.', got end of expression".to_string()
                            );
                        }
                    };
                    // Check if followed by ( -> method call, otherwise field access
                    if self.peek() == Some(&Token::LParen) {
                        self.advance(); // consume (
                        let args = self.parse_args()?;
                        self.expect(&Token::RParen)?;
                        expr = Expr::MethodCall {
                            receiver: Some(Box::new(expr)),
                            method: name,
                            args,
                        };
                    } else {
                        expr = Expr::FieldAccess(Box::new(expr), name);
                    }
                }
                Some(Token::LBracket) => {
                    self.advance(); // consume [
                    let index = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::IndexAccess(Box::new(expr), Box::new(index));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// Parse comma-separated arguments inside parentheses.
    fn parse_args(&mut self) -> Result<Vec<Expr>, String> {
        if self.peek() == Some(&Token::RParen) {
            return Ok(Vec::new());
        }
        let mut args = Vec::new();
        args.push(self.parse_expr()?);
        while self.peek() == Some(&Token::Comma) {
            self.advance(); // consume ,
            args.push(self.parse_expr()?);
        }
        Ok(args)
    }

    /// Parse primary expressions: literals, identifiers, parenthesized groups.
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::BoolLit(b)) => {
                let val = *b;
                self.advance();
                Ok(Expr::BoolLit(val))
            }
            Some(Token::NumberLit(n)) => {
                let val = *n;
                self.advance();
                Ok(Expr::NumberLit(val))
            }
            Some(Token::StringLit(_)) => {
                let tok = self.advance().unwrap();
                match tok {
                    Token::StringLit(s) => Ok(Expr::StringLit(s)),
                    _ => unreachable!(),
                }
            }
            Some(Token::Ident(_)) => {
                let tok = self.advance().unwrap();
                match tok {
                    Token::Ident(name) => {
                        // Check if followed by ( -> function call (no receiver)
                        if self.peek() == Some(&Token::LParen) {
                            self.advance(); // consume (
                            let args = self.parse_args()?;
                            self.expect(&Token::RParen)?;
                            Ok(Expr::MethodCall {
                                receiver: None,
                                method: name,
                                args,
                            })
                        } else {
                            Ok(Expr::Ident(name))
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Some(Token::LParen) => {
                self.advance(); // consume (
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Group(Box::new(inner)))
            }
            Some(t) => Err(format!("unexpected token {:?} in expression", t)),
            None => Err("unexpected end of expression".to_string()),
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Parse an expression string into an AST.
pub fn parse_expr(input: &str) -> Result<Expr, String> {
    let mut tokenizer = Tokenizer::new(input);
    let tokens = tokenizer.tokenize()?;
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    // Check that all tokens were consumed
    if parser.pos < parser.tokens.len() {
        let remaining = &parser.tokens[parser.pos..];
        return Err(format!("unexpected trailing tokens: {:?}", remaining));
    }
    Ok(expr)
}

/// Transform an expression AST into Rust code using `resolve()` calls.
///
/// In resolve mode, identifiers become truthy checks:
/// - `is_visible` → `(resolve("is_visible") == "true" || (!resolve("is_visible").is_empty() && resolve("is_visible") != "false"))`
/// - `count > 0` → `resolve("count").parse::<f64>().unwrap_or(0.0) > 0.0`
/// - `!is_hidden` → `!(truthy(resolve("is_hidden")))`
pub fn transform_expr_resolve(expr: &Expr) -> String {
    transform_expr(expr, TransformMode::Resolve)
}

/// Transform an expression AST into Rust code using `state.field.get()` references.
///
/// In state mode, identifiers become direct state references:
/// - `is_visible` → `state.is_visible.get()`
/// - `count > 0` → `state.count.get() > 0`
/// - `!is_hidden` → `!state.is_hidden.get()`
pub fn transform_expr_state(expr: &Expr) -> String {
    transform_expr(expr, TransformMode::State)
}

/// Backward-compatible wrapper: parse and transform in resolve mode.
/// This replaces the old `rewrite_if_expr` function.
pub fn rewrite_expr_resolve(input: &str) -> String {
    match parse_expr(input) {
        Ok(expr) => transform_expr_resolve(&expr),
        Err(_) => {
            // Fallback: if parsing fails, return the raw expression wrapped
            // in a basic truthy check (matches old behavior for edge cases)
            let trimmed = input.trim();
            if trimmed == "true"
                || trimmed == "false"
                || trimmed.contains('(')
                || trimmed.contains('.')
            {
                trimmed.to_string()
            } else {
                truthy_resolve(trimmed)
            }
        }
    }
}

// ── Transformer implementation ──────────────────────────────────────────────

fn string_lit(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn truthy_resolve(ident: &str) -> String {
    format!(
        "(resolve({key}) == \"true\" || (!resolve({key}).is_empty() && resolve({key}) != \"false\"))",
        key = string_lit(ident)
    )
}

fn transform_expr(expr: &Expr, mode: TransformMode) -> String {
    match expr {
        Expr::BoolLit(b) => b.to_string(),
        Expr::NumberLit(n) => {
            // Ensure f64 literal has decimal point for Rust
            if n.fract() == 0.0 {
                format!("{}.0", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Expr::StringLit(s) => string_lit(s),
        Expr::Ident(name) => transform_ident(name, mode, true),
        Expr::Not(inner) => format!("!{}", transform_expr(inner, mode)),
        Expr::Neg(inner) => format!("(-{})", transform_expr(inner, mode)),
        Expr::BinOp(kind, left, right) => {
            let op_str = bin_op_to_rust(*kind);
            // For == and !=, check if either side is a string literal.
            // If so, use string-based comparison for both sides.
            let is_string_eq = matches!(kind, BinOpKind::Eq | BinOpKind::Neq)
                && (expr_has_string_lit(left) || expr_has_string_lit(right));
            let left_str = transform_bin_operand(left, *kind, mode, is_string_eq);
            let right_str = transform_bin_operand(right, *kind, mode, is_string_eq);
            format!("({left_str} {op_str} {right_str})")
        }
        Expr::Group(inner) => format!("({})", transform_expr(inner, mode)),
        Expr::Ternary(cond, then_expr, else_expr) => {
            format!(
                "if {} {{ {} }} else {{ {} }}",
                transform_expr(cond, mode),
                transform_expr(then_expr, mode),
                transform_expr(else_expr, mode)
            )
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => {
            let args_str: Vec<String> = args.iter().map(|a| transform_expr(a, mode)).collect();
            match receiver {
                Some(recv) => {
                    let recv_str = transform_expr(recv, mode);
                    format!("{}.{}({})", recv_str, method, args_str.join(", "))
                }
                None => {
                    // Standalone function call
                    format!("{}({})", method, args_str.join(", "))
                }
            }
        }
        Expr::FieldAccess(recv, field) => {
            match mode {
                TransformMode::State => {
                    // In state mode, field access on state produces state.field.get()
                    // We detect `state.field` patterns specially
                    match recv.as_ref() {
                        Expr::Ident(name) if name == "state" => {
                            format!("state.{}.get()", field)
                        }
                        _ => format!("{}.{}", transform_expr(recv, mode), field),
                    }
                }
                TransformMode::Resolve => {
                    // In resolve mode, dot access becomes a single resolve key
                    // e.g., `items.length` → resolve("items.length")
                    // But for method calls and complex chains, we need different handling
                    let full_path = flatten_dot_path(expr);
                    if let Some(path) = full_path {
                        transform_ident(&path, mode, true)
                    } else {
                        format!("{}.{}", transform_expr(recv, mode), field)
                    }
                }
            }
        }
        Expr::IndexAccess(recv, index) => {
            format!(
                "{}[{}]",
                transform_expr(recv, mode),
                transform_expr(index, mode)
            )
        }
    }
}

/// Transform an identifier based on the mode.
/// `is_bool` indicates whether this ident is used in a boolean context.
fn transform_ident(name: &str, mode: TransformMode, is_bool: bool) -> String {
    match mode {
        TransformMode::Resolve => {
            // Special identifiers that should not be wrapped
            if name == "true" || name == "false" || name == "resolve" || name == "state" {
                name.to_string()
            } else if is_bool {
                truthy_resolve(name)
            } else {
                // Numeric context: resolve and parse as f64
                format!(
                    "resolve({}).parse::<f64>().unwrap_or(0.0)",
                    string_lit(name)
                )
            }
        }
        TransformMode::State => {
            // In state mode, identifiers reference state fields directly
            if name == "true" || name == "false" {
                name.to_string()
            } else if is_bool {
                // Boolean field on state: state.name.get()
                format!("state.{}.get()", name)
            } else {
                // Non-boolean field: state.name.get() (still use .get() for Signal fields)
                format!("state.{}.get()", name)
            }
        }
    }
}

/// For binary operators, determine how to transform each operand.
/// Comparison operators need numeric operands, logical operators need boolean.
fn transform_bin_operand(
    operand: &Expr,
    op_kind: BinOpKind,
    mode: TransformMode,
    is_string_eq: bool,
) -> String {
    let needs_numeric = matches!(
        op_kind,
        BinOpKind::Eq
            | BinOpKind::Neq
            | BinOpKind::Gt
            | BinOpKind::Lt
            | BinOpKind::Geq
            | BinOpKind::Leq
            | BinOpKind::Add
            | BinOpKind::Sub
            | BinOpKind::Mul
            | BinOpKind::Div
            | BinOpKind::Mod
    );

    if is_string_eq {
        // String comparison: keep identifiers as resolve() strings (no f64 parsing)
        match operand {
            Expr::Ident(name) => match mode {
                TransformMode::Resolve => format!("resolve({})", string_lit(name)),
                TransformMode::State => format!("state.{}.get()", name),
            },
            Expr::StringLit(s) => string_lit(s),
            _ => transform_expr(operand, mode),
        }
    } else if needs_numeric {
        // Numeric context: identifiers become f64 values
        match operand {
            Expr::Ident(name) => match mode {
                TransformMode::Resolve => format!(
                    "resolve({}).parse::<f64>().unwrap_or(0.0)",
                    string_lit(name)
                ),
                TransformMode::State => format!("state.{}.get()", name),
            },
            _ => transform_expr(operand, mode),
        }
    } else {
        // Boolean context (&&, ||): identifiers become truthy checks
        transform_expr(operand, mode)
    }
}

fn expr_has_string_lit(expr: &Expr) -> bool {
    match expr {
        Expr::StringLit(_) => true,
        Expr::Group(inner) => expr_has_string_lit(inner),
        _ => false,
    }
}

fn bin_op_to_rust(kind: BinOpKind) -> &'static str {
    match kind {
        BinOpKind::And => "&&",
        BinOpKind::Or => "||",
        BinOpKind::Eq => "==",
        BinOpKind::Neq => "!=",
        BinOpKind::Gt => ">",
        BinOpKind::Lt => "<",
        BinOpKind::Geq => ">=",
        BinOpKind::Leq => "<=",
        BinOpKind::Add => "+",
        BinOpKind::Sub => "-",
        BinOpKind::Mul => "*",
        BinOpKind::Div => "/",
        BinOpKind::Mod => "%",
    }
}

/// Flatten a chain of FieldAccess into a single dot-separated path string.
/// E.g., `state.count.value` → "state.count.value"
/// Returns None if the chain contains non-Ident nodes (e.g., method calls).
fn flatten_dot_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::FieldAccess(recv, field) => {
            let prefix = flatten_dot_path(recv)?;
            Some(format!("{}.{}", prefix, field))
        }
        _ => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser tests ──

    #[test]
    fn parse_simple_ident() {
        let expr = parse_expr("is_visible").unwrap();
        assert_eq!(expr, Expr::Ident("is_visible".to_string()));
    }

    #[test]
    fn parse_bool_literal() {
        assert_eq!(parse_expr("true").unwrap(), Expr::BoolLit(true));
        assert_eq!(parse_expr("false").unwrap(), Expr::BoolLit(false));
    }

    #[test]
    fn parse_number_literal() {
        assert_eq!(parse_expr("42").unwrap(), Expr::NumberLit(42.0));
        assert_eq!(parse_expr("3.14").unwrap(), Expr::NumberLit(3.14));
    }

    #[test]
    fn parse_string_literal() {
        let expr = parse_expr("\"hello\"").unwrap();
        assert_eq!(expr, Expr::StringLit("hello".to_string()));
    }

    #[test]
    fn parse_negation() {
        let expr = parse_expr("!is_hidden").unwrap();
        assert_eq!(
            expr,
            Expr::Not(Box::new(Expr::Ident("is_hidden".to_string())))
        );
    }

    #[test]
    fn parse_negation_parenthesized() {
        let expr = parse_expr("!(count > 0)").unwrap();
        assert_eq!(
            expr,
            Expr::Not(Box::new(Expr::Group(Box::new(Expr::BinOp(
                BinOpKind::Gt,
                Box::new(Expr::Ident("count".to_string())),
                Box::new(Expr::NumberLit(0.0))
            )))))
        );
    }

    #[test]
    fn parse_negation_bool() {
        assert_eq!(
            parse_expr("!true").unwrap(),
            Expr::Not(Box::new(Expr::BoolLit(true)))
        );
    }

    #[test]
    fn parse_logical_and() {
        let expr = parse_expr("is_visible && is_active").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::And,
                Box::new(Expr::Ident("is_visible".to_string())),
                Box::new(Expr::Ident("is_active".to_string()))
            )
        );
    }

    #[test]
    fn parse_logical_or() {
        let expr = parse_expr("is_visible || is_active").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::Or,
                Box::new(Expr::Ident("is_visible".to_string())),
                Box::new(Expr::Ident("is_active".to_string()))
            )
        );
    }

    #[test]
    fn parse_comparison() {
        let expr = parse_expr("count > 0").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::Gt,
                Box::new(Expr::Ident("count".to_string())),
                Box::new(Expr::NumberLit(0.0))
            )
        );
    }

    #[test]
    fn parse_comparison_with_logic() {
        let expr = parse_expr("count > 0 && is_visible").unwrap();
        // Precedence: && (2) is lower than > (4), so > binds tighter
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::And,
                Box::new(Expr::BinOp(
                    BinOpKind::Gt,
                    Box::new(Expr::Ident("count".to_string())),
                    Box::new(Expr::NumberLit(0.0))
                )),
                Box::new(Expr::Ident("is_visible".to_string()))
            )
        );
    }

    #[test]
    fn parse_combined_logic_and_negation() {
        let expr = parse_expr("is_visible && !is_hidden").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::And,
                Box::new(Expr::Ident("is_visible".to_string())),
                Box::new(Expr::Not(Box::new(Expr::Ident("is_hidden".to_string()))))
            )
        );
    }

    #[test]
    fn parse_parenthesized() {
        let expr = parse_expr("(count > 0)").unwrap();
        // Should be a Group containing a BinOp(Gt, Ident("count"), NumberLit(0))
        match expr {
            Expr::Group(inner) => match inner.as_ref() {
                Expr::BinOp(BinOpKind::Gt, _, _) => {}
                other => panic!("expected BinOp(Gt), got {:?}", other),
            },
            other => panic!("expected Group, got {:?}", other),
        }
    }

    #[test]
    fn parse_parens_override_precedence() {
        // Without parens: || binds lower than &&, so a && b || c = (a && b) || c
        // With parens: a || (b && c)
        let expr = parse_expr("a || (b && c)").unwrap();
        // Should be BinOp(Or, Ident("a"), Group(BinOp(And, ...)))
        match expr {
            Expr::BinOp(BinOpKind::Or, left, right) => {
                assert!(matches!(left.as_ref(), Expr::Ident(_)));
                assert!(matches!(right.as_ref(), Expr::Group(_)));
            }
            other => panic!("expected BinOp(Or), got {:?}", other),
        }
    }

    #[test]
    fn parse_ternary() {
        let expr = parse_expr("is_admin ? \"yes\" : \"no\"").unwrap();
        assert_eq!(
            expr,
            Expr::Ternary(
                Box::new(Expr::Ident("is_admin".to_string())),
                Box::new(Expr::StringLit("yes".to_string())),
                Box::new(Expr::StringLit("no".to_string()))
            )
        );
    }

    #[test]
    fn parse_ternary_with_comparison() {
        let expr = parse_expr("count > 10 ? \"high\" : \"low\"").unwrap();
        assert_eq!(
            expr,
            Expr::Ternary(
                Box::new(Expr::BinOp(
                    BinOpKind::Gt,
                    Box::new(Expr::Ident("count".to_string())),
                    Box::new(Expr::NumberLit(10.0))
                )),
                Box::new(Expr::StringLit("high".to_string())),
                Box::new(Expr::StringLit("low".to_string()))
            )
        );
    }

    #[test]
    fn parse_field_access() {
        let expr = parse_expr("state.count").unwrap();
        assert_eq!(
            expr,
            Expr::FieldAccess(
                Box::new(Expr::Ident("state".to_string())),
                "count".to_string()
            )
        );
    }

    #[test]
    fn parse_nested_field_access() {
        let expr = parse_expr("state.user.name").unwrap();
        assert_eq!(
            expr,
            Expr::FieldAccess(
                Box::new(Expr::FieldAccess(
                    Box::new(Expr::Ident("state".to_string())),
                    "user".to_string()
                )),
                "name".to_string()
            )
        );
    }

    #[test]
    fn parse_method_call() {
        let expr = parse_expr("items.len()").unwrap();
        assert_eq!(
            expr,
            Expr::MethodCall {
                receiver: Some(Box::new(Expr::Ident("items".to_string()))),
                method: "len".to_string(),
                args: vec![],
            }
        );
    }

    #[test]
    fn parse_method_call_with_args() {
        let expr = parse_expr("items.contains(42)").unwrap();
        assert_eq!(
            expr,
            Expr::MethodCall {
                receiver: Some(Box::new(Expr::Ident("items".to_string()))),
                method: "contains".to_string(),
                args: vec![Expr::NumberLit(42.0)],
            }
        );
    }

    #[test]
    fn parse_method_call_on_field() {
        let expr = parse_expr("state.items.len()").unwrap();
        assert_eq!(
            expr,
            Expr::MethodCall {
                receiver: Some(Box::new(Expr::FieldAccess(
                    Box::new(Expr::Ident("state".to_string())),
                    "items".to_string()
                ))),
                method: "len".to_string(),
                args: vec![],
            }
        );
    }

    #[test]
    fn parse_string_comparison() {
        let expr = parse_expr("name == \"admin\"").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::Eq,
                Box::new(Expr::Ident("name".to_string())),
                Box::new(Expr::StringLit("admin".to_string()))
            )
        );
    }

    #[test]
    fn parse_complex_expression() {
        let expr = parse_expr("count > 0 && is_visible || !is_hidden").unwrap();
        // Precedence: || (1) < && (2) < > (4)
        // count > 0 && is_visible || !is_hidden
        // = ((count > 0) && is_visible) || (!is_hidden)
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::Or,
                Box::new(Expr::BinOp(
                    BinOpKind::And,
                    Box::new(Expr::BinOp(
                        BinOpKind::Gt,
                        Box::new(Expr::Ident("count".to_string())),
                        Box::new(Expr::NumberLit(0.0))
                    )),
                    Box::new(Expr::Ident("is_visible".to_string()))
                )),
                Box::new(Expr::Not(Box::new(Expr::Ident("is_hidden".to_string()))))
            )
        );
    }

    #[test]
    fn parse_negation_of_complex() {
        let expr = parse_expr("!(a && b)").unwrap();
        // Should be Not(Group(BinOp(And, ...)))
        match expr {
            Expr::Not(inner) => {
                assert!(matches!(inner.as_ref(), Expr::Group(_)));
            }
            other => panic!("expected Not, got {:?}", other),
        }
    }

    #[test]
    fn parse_index_access() {
        let expr = parse_expr("items[0]").unwrap();
        assert_eq!(
            expr,
            Expr::IndexAccess(
                Box::new(Expr::Ident("items".to_string())),
                Box::new(Expr::NumberLit(0.0))
            )
        );
    }

    #[test]
    fn parse_multiple_comparisons() {
        let expr = parse_expr("age >= 18 && age <= 65").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOpKind::And,
                Box::new(Expr::BinOp(
                    BinOpKind::Geq,
                    Box::new(Expr::Ident("age".to_string())),
                    Box::new(Expr::NumberLit(18.0))
                )),
                Box::new(Expr::BinOp(
                    BinOpKind::Leq,
                    Box::new(Expr::Ident("age".to_string())),
                    Box::new(Expr::NumberLit(65.0))
                ))
            )
        );
    }

    #[test]
    fn parse_standalone_function_call() {
        let expr = parse_expr("is_valid()").unwrap();
        assert_eq!(
            expr,
            Expr::MethodCall {
                receiver: None,
                method: "is_valid".to_string(),
                args: vec![],
            }
        );
    }

    // ── Transform tests (resolve mode) ──

    #[test]
    fn transform_resolve_simple_ident() {
        let expr = parse_expr("is_visible").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("== \"true\""));
    }

    #[test]
    fn transform_resolve_logical_and() {
        let expr = parse_expr("is_visible && is_active").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("&&"));
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("resolve(\"is_active\")"));
    }

    #[test]
    fn transform_resolve_logical_or() {
        let expr = parse_expr("is_visible || is_active").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("||"));
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("resolve(\"is_active\")"));
    }

    #[test]
    fn transform_resolve_negation() {
        let expr = parse_expr("!is_hidden").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.starts_with("!"));
        assert!(out.contains("resolve(\"is_hidden\")"));
    }

    #[test]
    fn transform_resolve_negation_bool() {
        let expr = parse_expr("!true").unwrap();
        let out = transform_expr_resolve(&expr);
        assert_eq!(out, "!true");
    }

    #[test]
    fn transform_resolve_combined_logic() {
        let expr = parse_expr("is_visible && !is_hidden").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("&&"));
        assert!(out.contains("!"));
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("resolve(\"is_hidden\")"));
    }

    #[test]
    fn transform_resolve_comparison() {
        let expr = parse_expr("count > 0").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains(">"));
        assert!(out.contains("resolve(\"count\")"));
        assert!(out.contains("parse::<f64>()"));
    }

    #[test]
    fn transform_resolve_comparison_with_logic() {
        let expr = parse_expr("count > 0 && is_visible").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains(">"));
        assert!(out.contains("&&"));
        assert!(out.contains("resolve(\"count\")"));
        assert!(out.contains("parse::<f64>()"));
    }

    #[test]
    fn transform_resolve_string_comparison() {
        let expr = parse_expr("name == \"admin\"").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("resolve(\"name\")"));
        assert!(out.contains("\"admin\""));
        assert!(out.contains("=="));
        // Should NOT use parse::<f64>() for string comparisons
        assert!(!out.contains("parse::<f64>()"));
    }

    #[test]
    fn transform_resolve_ternary() {
        let expr = parse_expr("is_admin ? \"yes\" : \"no\"").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("if"));
        assert!(out.contains("\"yes\""));
        assert!(out.contains("\"no\""));
        assert!(out.contains("resolve(\"is_admin\")"));
    }

    #[test]
    fn transform_resolve_parenthesized() {
        let expr = parse_expr("(count > 0)").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("resolve(\"count\")"));
        assert!(out.contains(">"));
        assert!(out.contains("0.0"));
    }

    #[test]
    fn transform_resolve_negation_parenthesized() {
        let expr = parse_expr("!(count > 0)").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.starts_with("!"));
        assert!(out.contains("resolve(\"count\")"));
        assert!(out.contains(">"));
    }

    #[test]
    fn transform_resolve_method_call() {
        let expr = parse_expr("items.len()").unwrap();
        let out = transform_expr_resolve(&expr);
        // items.len() in resolve mode: resolve("items").len()
        assert!(out.contains("resolve(\"items\")"));
        assert!(out.contains(".len()"));
    }

    #[test]
    fn transform_resolve_method_call_comparison() {
        let expr = parse_expr("items.len() > 0").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains(".len()"));
        assert!(out.contains(">"));
        assert!(out.contains("0.0"));
    }

    #[test]
    fn transform_resolve_field_access() {
        let expr = parse_expr("state.count").unwrap();
        let out = transform_expr_resolve(&expr);
        // In resolve mode, dot paths become resolve("state.count")
        assert!(out.contains("resolve(\"state.count\")"));
    }

    #[test]
    fn transform_resolve_complex_expression() {
        let expr = parse_expr("count > 0 && is_visible || !is_hidden").unwrap();
        let out = transform_expr_resolve(&expr);
        assert!(out.contains("&&"));
        assert!(out.contains("||"));
        assert!(out.contains("!"));
        assert!(out.contains(">"));
        assert!(out.contains("resolve(\"count\")"));
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("resolve(\"is_hidden\")"));
    }

    // ── Transform tests (state mode) ──

    #[test]
    fn transform_state_simple_ident() {
        let expr = parse_expr("is_visible").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "state.is_visible.get()");
    }

    #[test]
    fn transform_state_negation() {
        let expr = parse_expr("!is_hidden").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "!state.is_hidden.get()");
    }

    #[test]
    fn transform_state_comparison() {
        let expr = parse_expr("count > 0").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "(state.count.get() > 0.0)");
    }

    #[test]
    fn transform_state_logical_and() {
        let expr = parse_expr("is_visible && is_active").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "(state.is_visible.get() && state.is_active.get())");
    }

    #[test]
    fn transform_state_field_access() {
        let expr = parse_expr("state.count").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "state.count.get()");
    }

    #[test]
    fn transform_state_nested_field() {
        let expr = parse_expr("state.user.name").unwrap();
        let out = transform_expr_state(&expr);
        // state.user.get().name — first .get() for Signal, then direct access
        // Actually, since we don't know types, we do state.user.get().name
        // But our transformer only special-cases state.X patterns
        // state.user -> state.user.get(), then .name stays as field access
        assert!(out.contains("state.user"));
        assert!(out.contains(".name"));
    }

    #[test]
    fn transform_state_ternary() {
        let expr = parse_expr("is_admin ? \"yes\" : \"no\"").unwrap();
        let out = transform_expr_state(&expr);
        assert!(out.contains("if state.is_admin.get()"));
        assert!(out.contains("\"yes\""));
        assert!(out.contains("\"no\""));
    }

    #[test]
    fn transform_state_method_call() {
        let expr = parse_expr("items.len()").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "state.items.get().len()");
    }

    #[test]
    fn transform_state_method_call_comparison() {
        let expr = parse_expr("items.len() > 0").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "(state.items.get().len() > 0.0)");
    }

    #[test]
    fn transform_state_string_comparison() {
        let expr = parse_expr("name == \"admin\"").unwrap();
        let out = transform_expr_state(&expr);
        assert_eq!(out, "(state.name.get() == \"admin\")");
    }

    // ── Backward compatibility (rewrite_expr_resolve) ──

    #[test]
    fn rewrite_compat_simple() {
        let out = rewrite_expr_resolve("is_visible");
        assert!(out.contains("resolve(\"is_visible\")"));
    }

    #[test]
    fn rewrite_compat_and() {
        let out = rewrite_expr_resolve("is_visible && is_active");
        assert!(out.contains("&&"));
        assert!(out.contains("resolve(\"is_visible\")"));
        assert!(out.contains("resolve(\"is_active\")"));
    }

    #[test]
    fn rewrite_compat_or() {
        let out = rewrite_expr_resolve("is_visible || is_active");
        assert!(out.contains("||"));
    }

    #[test]
    fn rewrite_compat_negation() {
        let out = rewrite_expr_resolve("!is_hidden");
        assert!(out.starts_with("!"));
        assert!(out.contains("resolve(\"is_hidden\")"));
    }

    #[test]
    fn rewrite_compat_comparison() {
        let out = rewrite_expr_resolve("count > 0");
        assert!(out.contains(">"));
        assert!(out.contains("parse::<f64>()"));
    }

    #[test]
    fn rewrite_compat_combined() {
        let out = rewrite_expr_resolve("count > 0 && is_visible");
        assert!(out.contains(">"));
        assert!(out.contains("&&"));
    }
}
