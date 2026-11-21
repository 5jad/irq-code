//! Structural parser for the first IRQ subset.
//! Expression parsing is provided for the MVP expression subset.

use irq_ast::{BinaryOp, Expression, Function, Module, Parameter, Statement, UnaryOp};
use irq_diagnostics::codes;
use irq_lexer::{Token, TokenKind};

pub const MAX_EXPRESSION_DEPTH: usize = 128;
pub const MAX_BLOCK_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub code: &'static str,
    pub line: usize,
    pub message: String,
    pub span: Option<irq_lexer::Span>,
}

/// Parse one expression with explicit precedence and associativity.
pub fn parse_expression(source: &str) -> Result<Expression, ParseError> {
    let tokens = irq_lexer::lex(source).map_err(|e| ParseError {
        code: e.code,
        line: e.span.line,
        message: e.message,
        span: Some(e.span),
    })?;
    let mut p = ExprParser {
        source: source.to_owned(),
        tokens,
        pos: 0,
        depth: 0,
    };
    let expr = p.expression(0)?;
    if !matches!(p.peek(), TokenKind::Eof | TokenKind::Newline) {
        return Err(p.err("unexpected token after expression"));
    }
    Ok(expr)
}

struct ExprParser {
    source: String,
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}
impl ExprParser {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }
    fn take(&mut self) -> TokenKind {
        let t = self.tokens[self.pos].kind.clone();
        if !matches!(t, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }
    fn err(&self, message: &str) -> ParseError {
        ParseError {
            code: codes::EXPRESSION_ERROR,
            line: self.tokens[self.pos.min(self.tokens.len() - 1)].span.line,
            message: message.into(),
            span: Some(self.tokens[self.pos.min(self.tokens.len() - 1)].span),
        }
    }
    fn expression(&mut self, min_bp: u8) -> Result<Expression, ParseError> {
        if self.depth >= MAX_EXPRESSION_DEPTH {
            return Err(ParseError {
                code: codes::NESTING_LIMIT,
                line: self.tokens[self.pos.min(self.tokens.len() - 1)].span.line,
                message: format!(
                    "expression nesting exceeds the {MAX_EXPRESSION_DEPTH} level limit"
                ),
                span: Some(self.tokens[self.pos.min(self.tokens.len() - 1)].span),
            });
        }
        self.depth += 1;
        let result = self.expression_inner(min_bp);
        self.depth -= 1;
        result
    }

    fn expression_inner(&mut self, min_bp: u8) -> Result<Expression, ParseError> {
        let mut lhs = match self.take() {
            TokenKind::Number(n) => Expression::Integer(
                n.parse()
                    .map_err(|_| self.err("integer literal is out of range"))?,
            ),
            TokenKind::Ident(ref n) if n == "true" => Expression::Boolean(true),
            TokenKind::Ident(ref n) if n == "false" => Expression::Boolean(false),
            TokenKind::String(value) => Expression::String(value),
            TokenKind::Ident(ref n) if n == "not" => Expression::Unary {
                op: UnaryOp::Not,
                value: Box::new(self.expression(11)?),
            },
            TokenKind::Ident(ref n) if is_reserved(n) => {
                return Err(self.err(&format!("reserved word `{n}` cannot be used as a name")));
            }
            TokenKind::Ident(n) => Expression::Name(n),
            TokenKind::Minus => Expression::Unary {
                op: UnaryOp::Negate,
                value: Box::new(self.expression(11)?),
            },
            TokenKind::LParen => {
                if matches!(self.peek(), TokenKind::RParen) {
                    self.take();
                    return Ok(Expression::Unit);
                }
                let e = self.expression(0)?;
                if !matches!(self.take(), TokenKind::RParen) {
                    return Err(self.err("expected `)`"));
                }
                e
            }
            _ => return Err(self.err("expected expression")),
        };
        let mut comparison_seen = false;
        loop {
            if matches!(self.peek(), TokenKind::LParen) {
                let Expression::Name(callee) = lhs else {
                    return Err(self.err("only a named function can be called in this subset"));
                };
                self.take();
                let mut arguments = Vec::new();
                if !matches!(self.peek(), TokenKind::RParen) {
                    loop {
                        arguments.push(self.expression(0)?);
                        if matches!(self.peek(), TokenKind::Comma) {
                            let comma = self.tokens[self.pos].span;
                            self.take();
                            let next = self.tokens[self.pos].span;
                            self.require_spaces(
                                comma.end,
                                next.start,
                                "expected a space after `,`",
                            )?;
                            continue;
                        }
                        break;
                    }
                }
                if !matches!(self.take(), TokenKind::RParen) {
                    return Err(self.err("expected `)` after function arguments"));
                }
                lhs = Expression::Call { callee, arguments };
                continue;
            }
            let (op, l, r) = match self.peek() {
                TokenKind::Ident(s) if s == "or" => (BinaryOp::Or, 1, 2),
                TokenKind::Ident(s) if s == "and" => (BinaryOp::And, 3, 4),
                TokenKind::EqEq => (BinaryOp::Equal, 5, 6),
                TokenKind::NotEq => (BinaryOp::NotEqual, 5, 6),
                TokenKind::Lt => (BinaryOp::Less, 5, 6),
                TokenKind::Le => (BinaryOp::LessEqual, 5, 6),
                TokenKind::Gt => (BinaryOp::Greater, 5, 6),
                TokenKind::Ge => (BinaryOp::GreaterEqual, 5, 6),
                TokenKind::Plus => (BinaryOp::Add, 7, 8),
                TokenKind::Minus => (BinaryOp::Subtract, 7, 8),
                TokenKind::Star => (BinaryOp::Multiply, 9, 10),
                TokenKind::Slash => (BinaryOp::Divide, 9, 10),
                TokenKind::Percent => (BinaryOp::Remainder, 9, 10),
                _ => break,
            };
            if matches!(op, BinaryOp::And | BinaryOp::Or) {
                let current = self.tokens[self.pos].span;
                let previous = self.tokens[self.pos - 1].span;
                let next = self.tokens[self.pos + 1].span;
                self.require_spaces(
                    previous.end,
                    current.start,
                    "expected a space before logical operator",
                )?;
                self.require_spaces(
                    current.end,
                    next.start,
                    "expected a space after logical operator",
                )?;
            }
            if matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
            ) {
                if comparison_seen {
                    return Err(self.err("chained comparisons are not allowed; add parentheses"));
                }
                comparison_seen = true;
            }
            if l < min_bp {
                break;
            }
            self.take();
            let rhs = self.expression(r)?;
            lhs = Expression::Binary {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn require_spaces(&self, start: usize, end: usize, message: &str) -> Result<(), ParseError> {
        if start < end
            && self
                .source
                .get(start..end)
                .is_some_and(|gap| gap.chars().all(|ch| ch == ' '))
        {
            Ok(())
        } else {
            Err(self.err(message))
        }
    }
}

pub fn parse(source: &str) -> Result<Module, ParseError> {
    parse_inner(source).map_err(|mut parse_error| {
        if parse_error.span.is_none() {
            parse_error.span = Some(line_span(source, parse_error.line));
        }
        parse_error
    })
}

fn parse_inner(source: &str) -> Result<Module, ParseError> {
    irq_lexer::lex(source).map_err(|e| ParseError {
        code: e.code,
        line: e.span.line,
        message: e.message,
        span: Some(e.span),
    })?;
    for (index, raw) in source.lines().enumerate() {
        let prefix: String = raw
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect();
        if prefix.contains('\t') {
            return Err(error(index + 1, "tabs are not allowed in indentation"));
        }
    }
    let rows: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .map(|(i, s)| (i + 1, strip_comment(s).trim_end()))
        .filter(|(_, s)| !s.trim().is_empty())
        .collect();
    let Some((mod_line, first)) = rows.first().copied() else {
        return Err(error(1, "file is empty; expected `mod name`"));
    };
    if first.starts_with(' ') || first.starts_with('\t') {
        return Err(error(mod_line, "module declaration must not be indented"));
    }
    let Some(module_name) = first.strip_prefix("mod ") else {
        return Err(error(mod_line, "file must start with `mod name`"));
    };
    let name = module_name.trim();
    if !valid_ident(name) {
        return Err(error(mod_line, "invalid module declaration"));
    }

    let mut functions = Vec::new();
    let mut i = 1;
    while i < rows.len() {
        let (line, text) = rows[i];
        let trimmed = text.trim();
        if text.starts_with(' ') || text.starts_with('\t') {
            return Err(error(line, "function declaration must not be indented"));
        }
        let signature = if let Some(s) = trimmed.strip_prefix("pub fn ") {
            s
        } else if let Some(s) = trimmed.strip_prefix("fn ") {
            s
        } else {
            return Err(error(
                line,
                "expected function declaration `fn name(...) -> Type`",
            ));
        };
        let open = signature
            .find('(')
            .ok_or_else(|| error(line, "expected `(` after function name"))?;
        let fn_name = &signature[..open];
        if !valid_ident(fn_name) {
            return Err(error(line, "invalid function name"));
        }
        let close = matching_paren(signature, open)
            .ok_or_else(|| error(line, "expected `)` in function signature"))?;
        let return_tail = signature[close + 1..].trim_start_matches(' ');
        if return_tail == &signature[close + 1..]
            || !return_tail.starts_with("->")
            || !return_tail[2..].starts_with(' ')
        {
            return Err(error(line, "function signature must use `) -> ReturnType`"));
        }
        let params_text = &signature[open + 1..close];
        let mut parameters = Vec::new();
        if !params_text.trim().is_empty() {
            for (parameter_index, raw_param) in params_text.split(',').enumerate() {
                let param = if parameter_index == 0 {
                    raw_param
                } else {
                    raw_param.trim_start_matches(' ')
                };
                if parameter_index > 0 && !raw_param.starts_with(' ') {
                    return Err(error(line, "parameters after `,` require a space"));
                }
                if raw_param.contains('\t') {
                    return Err(error(line, "tabs are not allowed in parameter separators"));
                }
                let (param_name, param_ty) = param
                    .split_once(':')
                    .ok_or_else(|| error(line, "each parameter requires `name: Type`"))?;
                if param_name.trim() != param_name {
                    return Err(error(
                        line,
                        "parameter name must not contain surrounding spaces",
                    ));
                }
                if !param_ty.starts_with(' ') || param_ty.trim_end() != param_ty {
                    return Err(error(
                        line,
                        "parameter type requires a space after `:` and none before `,`",
                    ));
                }
                let param_name = param_name.trim();
                let param_ty = param_ty.trim();
                if !valid_ident(param_name) || param_ty.is_empty() {
                    return Err(error(
                        line,
                        "invalid parameter name or missing parameter type",
                    ));
                }
                if parameters.iter().any(|p: &Parameter| p.name == param_name) {
                    return Err(error(line, "duplicate parameter name"));
                }
                parameters.push(Parameter {
                    name: param_name.to_owned(),
                    ty: param_ty.to_owned(),
                });
            }
        }
        let return_tail = return_tail[2..].trim();
        let return_type = return_tail
            .split("| err(")
            .next()
            .unwrap_or(return_tail)
            .trim();
        if return_type.is_empty() {
            return Err(error(line, "return type is missing"));
        }

        let mut sections = Vec::new();
        let mut body_index = None;
        i += 1;
        while i < rows.len() {
            let section = rows[i].1.trim();
            if section.starts_with("fn ") || section.starts_with("pub fn ") {
                break;
            }
            if ["reads:", "writes:", "requires:", "ensures:", "body:"]
                .iter()
                .any(|key| section.starts_with(*key))
            {
                let indent = rows[i].1.len() - rows[i].1.trim_start_matches(' ').len();
                if indent != 2 {
                    return Err(error(
                        rows[i].0,
                        "function signature sections must use exactly two leading spaces",
                    ));
                }
                let (section_name, section_value) = section.split_once(':').unwrap();
                let section_value = section_value.trim();
                if sections
                    .iter()
                    .any(|(existing, _)| *existing == section_name)
                {
                    return Err(error(
                        rows[i].0,
                        &format!("duplicate `{section_name}:` section"),
                    ));
                }
                match section_name {
                    "reads" | "writes" if section_value != "none" => {
                        return Err(error(
                            rows[i].0,
                            "IRQ 0.1 permits only `none` in reads/writes sections",
                        ));
                    }
                    "requires" | "ensures" => {
                        let expression = section_value;
                        parse_expression(expression)
                            .map_err(|e| map_expression_error(e, expression, source, rows[i].0))?;
                    }
                    "body" if !section_value.is_empty() => {
                        return Err(error(rows[i].0, "`body:` section header must be empty"));
                    }
                    _ => {}
                }
                let key = section_name;
                sections.push((key, rows[i].0));
                if key == "body" {
                    body_index = Some(i);
                }
            }
            i += 1;
        }
        for required in ["reads", "writes", "requires", "ensures", "body"] {
            if !sections.iter().any(|(key, _)| *key == required) {
                return Err(error(
                    line,
                    &format!("function `{fn_name}` is missing `{required}:` section"),
                ));
            }
        }
        let mut previous = 0;
        for (key, section_line) in &sections {
            let rank = match *key {
                "reads" => 1,
                "writes" => 2,
                "requires" => 3,
                "ensures" => 4,
                "body" => 5,
                _ => 0,
            };
            if rank < previous {
                return Err(error(*section_line, "function sections are out of order"));
            }
            previous = rank;
        }
        let body_index = body_index.ok_or_else(|| error(line, "body section missing"))?;
        let mut body_cursor = body_index + 1;
        let body = parse_block(&rows, &mut body_cursor, 4, source)?;
        if body.is_empty() {
            return Err(error(
                rows[body_index].0,
                "function body must contain at least one supported statement",
            ));
        }
        if body_cursor < rows.len() {
            let (next_line, next_text) = rows[body_cursor];
            let next_trimmed = next_text.trim_start();
            let is_next_function = next_text.len() == next_trimmed.len()
                && (next_trimmed.starts_with("fn ") || next_trimmed.starts_with("pub fn "));
            if !is_next_function {
                return Err(error(
                    next_line,
                    "unexpected content outside a function body",
                ));
            }
        }
        functions.push(Function {
            name: fn_name.to_owned(),
            signature: trimmed.to_owned(),
            line,
            parameters,
            return_type: return_type.to_owned(),
            body,
        });
    }
    if functions.is_empty() {
        return Err(error(mod_line, "module must contain at least one function"));
    }
    Ok(Module {
        name: name.to_owned(),
        functions,
    })
}

fn line_span(source: &str, target_line: usize) -> irq_lexer::Span {
    let mut offset = 0usize;
    for (index, segment) in source.split_inclusive('\n').enumerate() {
        if index + 1 == target_line {
            let mut end = offset + segment.len();
            if segment.ends_with('\n') {
                end -= 1;
                if end > offset && source.as_bytes()[end - 1] == b'\r' {
                    end -= 1;
                }
            }
            return irq_lexer::Span {
                start: offset,
                end,
                line: target_line,
                column: 1,
            };
        }
        offset += segment.len();
    }
    irq_lexer::Span {
        start: source.len(),
        end: source.len(),
        line: target_line,
        column: 1,
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (offset, ch) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '#' {
            return &line[..offset];
        }
    }
    line
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (relative, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + relative);
                }
            }
            _ => {}
        }
    }
    None
}

fn valid_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(x) if x.is_ascii_alphabetic() || x == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        && !is_reserved(s)
}

fn is_reserved(s: &str) -> bool {
    matches!(
        s,
        "mod"
            | "pub"
            | "fn"
            | "reads"
            | "writes"
            | "requires"
            | "ensures"
            | "body"
            | "none"
            | "let"
            | "mut"
            | "if"
            | "else"
            | "while"
            | "return"
            | "or"
            | "and"
            | "not"
            | "true"
            | "false"
            | "i32"
            | "u32"
            | "i64"
            | "u64"
            | "bool"
            | "str"
            | "Result"
            | "use"
            | "const"
            | "struct"
            | "enum"
            | "errdef"
            | "test"
            | "for"
            | "break"
            | "old"
            | "null"
            | "opt"
            | "match"
            | "try"
            | "ok"
            | "err"
    )
}

fn parse_block(
    rows: &[(usize, &str)],
    cursor: &mut usize,
    expected_indent: usize,
    full_source: &str,
) -> Result<Vec<Statement>, ParseError> {
    if expected_indent > 4 + MAX_BLOCK_DEPTH * 2 {
        return Err(ParseError {
            code: codes::NESTING_LIMIT,
            line: rows.get(*cursor).map(|row| row.0).unwrap_or(1),
            message: format!("block nesting exceeds the {MAX_BLOCK_DEPTH} level limit"),
            span: None,
        });
    }
    let mut statements = Vec::new();
    while *cursor < rows.len() {
        let (line, text) = rows[*cursor];
        let trimmed = text.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") || trimmed == "else:" {
            break;
        }
        let indent = text.chars().take_while(|c| *c == ' ').count();
        if indent < expected_indent {
            break;
        }
        if indent > expected_indent {
            return Err(error(line, "unexpected indentation level"));
        }
        if let Some(condition) = trimmed
            .strip_prefix("if ")
            .and_then(|s| s.strip_suffix(':'))
        {
            let condition = condition.trim();
            let condition = parse_expression(condition)
                .map_err(|e| map_expression_error(e, condition, full_source, line))?;
            *cursor += 1;
            let then_body = parse_block(rows, cursor, expected_indent + 2, full_source)?;
            if then_body.is_empty() {
                return Err(error(line, "if branch must not be empty"));
            }
            let Some((else_line, else_text)) = rows.get(*cursor).copied() else {
                return Err(error(
                    line,
                    "if requires an `else:` branch in this MVP subset",
                ));
            };
            let else_indent = else_text.chars().take_while(|c| *c == ' ').count();
            if else_indent != expected_indent || else_text.trim() != "else:" {
                return Err(error(
                    else_line,
                    "if requires an `else:` branch at the same indentation",
                ));
            }
            *cursor += 1;
            let else_body = parse_block(rows, cursor, expected_indent + 2, full_source)?;
            if else_body.is_empty() {
                return Err(error(else_line, "else branch must not be empty"));
            }
            statements.push(Statement::If {
                condition,
                then_body,
                else_body,
            });
        } else if let Some(condition) = trimmed
            .strip_prefix("while ")
            .and_then(|s| s.strip_suffix(':'))
        {
            let condition = condition.trim();
            let condition = parse_expression(condition)
                .map_err(|e| map_expression_error(e, condition, full_source, line))?;
            *cursor += 1;
            let body = parse_block(rows, cursor, expected_indent + 2, full_source)?;
            if body.is_empty() {
                return Err(error(line, "while body must not be empty"));
            }
            statements.push(Statement::While { condition, body });
        } else {
            statements.push(parse_statement(trimmed, line, full_source)?);
            *cursor += 1;
        }
    }
    Ok(statements)
}

fn parse_statement(source: &str, line: usize, full_source: &str) -> Result<Statement, ParseError> {
    if let Some(expr) = source.strip_prefix("return ") {
        return Ok(Statement::Return(
            parse_expression(expr).map_err(|e| map_expression_error(e, expr, full_source, line))?,
        ));
    }
    if let Some(decl) = source.strip_prefix("let ") {
        let (lhs, rhs) = decl
            .split_once('=')
            .ok_or_else(|| error(line, "expected `=` in let declaration"))?;
        let lhs = lhs.trim();
        let (mutable, typed_name) = if let Some(rest) = lhs.strip_prefix("mut ") {
            (true, rest.trim())
        } else {
            (false, lhs)
        };
        let (name, ty) = typed_name
            .split_once(':')
            .ok_or_else(|| error(line, "let declaration requires an explicit type"))?;
        let name = name.trim();
        let ty = ty.trim();
        if !valid_ident(name) || ty.is_empty() {
            return Err(error(line, "invalid let name or missing type"));
        }
        let rhs = rhs.trim();
        let value =
            parse_expression(rhs).map_err(|e| map_expression_error(e, rhs, full_source, line))?;
        return Ok(Statement::Let {
            name: name.to_owned(),
            mutable,
            ty: ty.to_owned(),
            value,
        });
    }
    if let Some((lhs, rhs)) = source.split_once('=')
        && !rhs.starts_with('=')
    {
        let name = lhs.trim();
        if valid_ident(name) {
            let rhs = rhs.trim();
            let value = parse_expression(rhs)
                .map_err(|e| map_expression_error(e, rhs, full_source, line))?;
            return Ok(Statement::Assign {
                name: name.to_owned(),
                value,
            });
        }
    }
    Err(error(
        line,
        "unsupported statement in current parser subset",
    ))
}

fn map_expression_error(
    mut error: ParseError,
    expression: &str,
    full_source: &str,
    line: usize,
) -> ParseError {
    error.line = line;
    if let Some(mut span) = error.span {
        let base = (expression.as_ptr() as usize).saturating_sub(full_source.as_ptr() as usize);
        let base = base.min(full_source.len());
        let line_start = full_source[..base]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let base_column = full_source[line_start..base].chars().count() + 1;
        span.start += base;
        span.end += base;
        span.line = line;
        span.column += base_column - 1;
        error.span = Some(span);
    }
    error
}

fn error(line: usize, message: &str) -> ParseError {
    ParseError {
        code: codes::PARSE_ERROR,
        line,
        message: message.to_owned(),
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID: &str = "mod demo\n\npub fn main() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: result == 0\n  body:\n    return 0\n";
    #[test]
    fn parses_module_and_function() {
        let m = parse(VALID).unwrap();
        assert_eq!(m.name, "demo");
        assert_eq!(m.functions[0].name, "main");
        assert!(matches!(
            m.functions[0].body.as_slice(),
            [Statement::Return(Expression::Integer(0))]
        ));
    }
    #[test]
    fn rejects_missing_contract_section() {
        let source = VALID.replace("  ensures: result == 0\n", "");
        assert!(parse(&source).unwrap_err().message.contains("ensures"));
    }

    #[test]
    fn rejects_duplicate_function_sections() {
        let source = VALID.replace("  writes: none\n", "  reads: fs\n  writes: none\n");
        assert!(
            parse(&source)
                .unwrap_err()
                .message
                .contains("duplicate `reads:` section")
        );
    }

    #[test]
    fn validates_effect_contract_and_body_section_grammar() {
        for source in [
            VALID.replace("reads: none", "reads: filesystem"),
            VALID.replace("writes: none", "writes: network"),
            VALID.replace("requires: true", "requires: true and"),
            VALID.replace("  body:", "  body: unexpected"),
        ] {
            assert!(
                parse(&source).is_err(),
                "accepted invalid section:\n{source}"
            );
        }
        assert!(parse(VALID).is_ok());
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let parsed = parse_expression("1 + 2 * 3").unwrap();
        assert!(
            matches!(parsed, Expression::Binary { op: BinaryOp::Add, ref right, .. }
            if matches!(**right, Expression::Binary { op: BinaryOp::Multiply, .. }))
        );
    }

    #[test]
    fn rejects_chained_comparisons_but_accepts_explicit_parenthesization() {
        for expression in ["a < b < c", "a == b != c", "a <= b > c"] {
            let error = parse_expression(expression).unwrap_err();
            assert!(
                error.message.contains("chained comparisons"),
                "{expression}"
            );
            assert!(error.span.is_some());
        }
        assert!(parse_expression("(a < b) == true").is_ok());
    }

    #[test]
    fn enforces_required_spaces_around_logical_operators_and_after_commas() {
        assert!(parse_expression("left or right").is_ok());
        assert!(parse_expression("left  and   right").is_ok());
        for expression in ["left or(right)", "left\tor\tright", "left andright"] {
            assert!(parse_expression(expression).is_err(), "{expression}");
        }
        assert!(parse_expression("call(1, 2)").is_ok());
        assert!(parse_expression("call(1,2)").is_err());
    }

    #[test]
    fn remainder_shares_multiplicative_precedence_and_is_left_associative() {
        let parsed = parse_expression("20 % 6 * 2").unwrap();
        assert!(
            matches!(parsed, Expression::Binary { op: BinaryOp::Multiply, ref left, .. }
            if matches!(**left, Expression::Binary { op: BinaryOp::Remainder, .. }))
        );
        let chained = parse_expression("20 % 6 % 4").unwrap();
        assert!(
            matches!(chained, Expression::Binary { op: BinaryOp::Remainder, ref left, .. }
            if matches!(**left, Expression::Binary { op: BinaryOp::Remainder, .. }))
        );
    }

    #[test]
    fn rejects_incomplete_expression() {
        assert!(parse_expression("1 +").is_err());
        assert!(parse_expression("true false").is_err());
    }

    #[test]
    fn parses_unit_expression_and_grouped_expression_separately() {
        assert_eq!(parse_expression("()").unwrap(), Expression::Unit);
        assert_eq!(
            parse_expression("(1 + 2)").unwrap(),
            parse_expression("1 + 2").unwrap()
        );
    }

    #[test]
    fn embedded_expression_errors_carry_absolute_file_spans() {
        let source = VALID.replace("return 0", "return 0 +");
        let error = parse(&source).unwrap_err();
        assert_eq!(error.line, 9);
        let span = error.span.unwrap();
        let expected = source.find("0 +").unwrap() + 3;
        assert_eq!(span.start, expected);
        assert_eq!(span.end, expected);
        assert_eq!(span.line, 9);
    }

    #[test]
    fn embedded_expression_error_after_unicode_uses_absolute_utf8_offset() {
        let source = VALID.replace("return 0", "return \"مرحبا\" +");
        let error = parse(&source).unwrap_err();
        let span = error.span.unwrap();
        let expected = source.find('+').unwrap() + 1;
        assert_eq!(span.start, expected);
        assert_eq!(span.end, expected);
        assert_eq!(span.line, 9);
    }

    #[test]
    fn parses_local_declaration_assignment_and_return() {
        let source = "mod demo\nfn calc() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut total: i32 = 1 + 2 * 3\n    total = total + 1\n    return total\n";
        let module = parse(source).unwrap();
        assert_eq!(module.functions[0].body.len(), 3);
        assert!(matches!(
            module.functions[0].body[0],
            Statement::Let { mutable: true, .. }
        ));
        assert!(matches!(
            module.functions[0].body[1],
            Statement::Assign { .. }
        ));
    }

    #[test]
    fn parses_unicode_string_literals_and_escapes() {
        let expression = parse_expression("\"مرحبا\\nIRQ\"").unwrap();
        assert_eq!(expression, Expression::String("مرحبا\nIRQ".into()));
    }

    #[test]
    fn hash_inside_string_is_not_treated_as_comment() {
        let source = VALID
            .replace("-> i32", "-> str")
            .replace("ensures: result == 0", "ensures: true")
            .replace("return 0", "return \"left#right\"");
        let module = parse(&source).unwrap();
        assert!(matches!(
            module.functions[0].body.as_slice(),
            [Statement::Return(Expression::String(value))] if value == "left#right"
        ));
    }

    #[test]
    fn rejects_reserved_words_as_module_function_parameter_and_local_names() {
        assert!(
            parse(&VALID.replace("mod demo", "mod return"))
                .unwrap_err()
                .message
                .contains("invalid module declaration")
        );
        assert!(
            parse(&VALID.replace("fn main()", "fn return()"))
                .unwrap_err()
                .message
                .contains("invalid function name")
        );
        let parameter = VALID.replace("main()", "main(return: i32)");
        assert!(
            parse(&parameter)
                .unwrap_err()
                .message
                .contains("invalid parameter name")
        );
        let local = VALID.replace("    return 0", "    let mut: i32 = 0\n    return 0");
        assert!(parse(&local).is_err());
        assert!(
            parse_expression("match")
                .unwrap_err()
                .message
                .contains("reserved word")
        );
    }

    #[test]
    fn rejects_every_reserved_word_as_a_function_identifier() {
        let reserved = [
            "mod", "pub", "fn", "reads", "writes", "requires", "ensures", "body", "none", "let",
            "mut", "if", "else", "while", "return", "or", "and", "not", "true", "false", "i32",
            "u32", "i64", "u64", "bool", "str", "Result", "use", "const", "struct", "enum",
            "errdef", "test", "for", "break", "old", "null", "opt", "match", "try", "ok", "err",
        ];
        for word in reserved {
            let source = VALID.replace("fn main()", &format!("fn {word}()"));
            assert!(
                parse(&source).is_err(),
                "reserved function name accepted: {word}"
            );
        }
        assert!(parse(&VALID.replace("fn main()", "fn return_value()")).is_ok());
    }

    #[test]
    fn requires_module_function_and_signature_sections_at_canonical_indentation() {
        assert!(
            parse(&VALID.replace("mod demo", "  mod demo"))
                .unwrap_err()
                .message
                .contains("module declaration must not be indented")
        );
        assert!(
            parse(&VALID.replace("pub fn main()", "  pub fn main()"))
                .unwrap_err()
                .message
                .contains("function declaration must not be indented")
        );
        assert!(
            parse(&VALID.replace("  reads:", "    reads:"))
                .unwrap_err()
                .message
                .contains("exactly two leading spaces")
        );
        assert!(
            parse(&VALID.replace("  reads:", "\treads:"))
                .unwrap_err()
                .message
                .contains("tabs are not allowed")
        );
        assert!(parse(&VALID.replace("mod demo", "mod\tdemo")).is_err());
    }

    #[test]
    fn enforces_signature_separator_spaces_from_the_ebnf() {
        for signature in [
            "fn calc(a:i32) -> i32",
            "fn calc(a : i32) -> i32",
            "fn calc(a: i32,b: i32) -> i32",
            "fn calc() ->i32",
            "fn calc()-> i32",
        ] {
            let source = VALID.replace("pub fn main() -> i32", signature);
            assert!(parse(&source).is_err(), "accepted signature `{signature}`");
        }
        let accepted = VALID.replace(
            "pub fn main() -> i32",
            "pub fn calc(a:  i32,  b: i32)  ->  i32",
        );
        assert!(parse(&accepted).is_ok());
    }

    #[test]
    fn rejects_unsupported_body_statement() {
        let source = VALID.replace("    return 0", "    print(0)");
        assert!(
            parse(&source)
                .unwrap_err()
                .message
                .contains("unsupported statement")
        );
    }

    #[test]
    fn parses_if_else_return_branches() {
        let source = "mod demo\nfn sign(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if x > 0:\n      return 1\n    else:\n      return 0\n";
        let module = parse(source).unwrap();
        assert!(matches!(
            module.functions[0].body.as_slice(),
            [Statement::If { .. }]
        ));
    }

    #[test]
    fn parses_while_with_nested_body() {
        let source = "mod demo\nfn count() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut n: i32 = 0\n    while n < 3:\n      n = n + 1\n    return n\n";
        let module = parse(source).unwrap();
        assert!(matches!(
            module.functions[0].body.as_slice(),
            [Statement::Let { .. }, Statement::While { body, .. }, Statement::Return(_)]
                if matches!(body.as_slice(), [Statement::Assign { .. }])
        ));
    }

    #[test]
    fn rejects_empty_while_body() {
        let source = "mod demo\nfn count() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    while true:\n    return 0\n";
        assert!(
            parse(source)
                .unwrap_err()
                .message
                .contains("while body must not be empty")
        );
    }

    #[test]
    fn rejects_excessively_nested_expressions_with_stable_code() {
        let source = format!(
            "{}1{}",
            "(".repeat(MAX_EXPRESSION_DEPTH + 1),
            ")".repeat(MAX_EXPRESSION_DEPTH + 1)
        );
        let error = parse_expression(&source).unwrap_err();
        assert_eq!(error.code, codes::NESTING_LIMIT);
    }

    #[test]
    fn rejects_excessively_nested_blocks_with_stable_code() {
        let mut source = String::from(
            "mod deep\nfn f() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n",
        );
        for level in 0..=MAX_BLOCK_DEPTH {
            source.push_str(&" ".repeat(4 + 2 * level));
            source.push_str("while true:\n");
        }
        source.push_str(&" ".repeat(4 + 2 * (MAX_BLOCK_DEPTH + 1)));
        source.push_str("return 0\n");
        let error = parse(&source).unwrap_err();
        assert_eq!(error.code, codes::NESTING_LIMIT);
    }

    #[test]
    fn rejects_oversized_module_before_structural_parsing() {
        let source = format!("{}{}", VALID, " ".repeat(irq_lexer::MAX_SOURCE_BYTES));
        let error = parse(&source).unwrap_err();
        assert_eq!(error.code, codes::SOURCE_TOO_LARGE);
    }

    #[test]
    fn structural_errors_receive_utf8_byte_spans_on_crlf_sources() {
        let source = "# تعليق عربي\r\nmod 123\r\n";
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("invalid module declaration"));
        let span = error.span.unwrap();
        assert_eq!(span.start, "# تعليق عربي\r\n".len());
        assert_eq!(span.end, span.start + "mod 123".len());
        assert_eq!(span.line, 2);
        assert_eq!(span.column, 1);

        let source = "mod demo\r\nfn missing_arrow() i32\r\n";
        let error = parse(source).unwrap_err();
        let span = error.span.unwrap();
        assert_eq!(span.start, "mod demo\r\n".len());
        assert_eq!(span.end, source.trim_end_matches("\r\n").len());
        assert_eq!(span.line, 2);
    }

    #[test]
    fn deterministic_fuzz_smoke_never_panics() {
        let alphabet = [
            'a', '1', '_', ' ', '\n', '#', '"', '\\', 'ع', '(', '+', '-', '/', '\t',
        ];
        let mut state = 0xc0ffee_u64;
        for _ in 0..1_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let length = (state as usize) % 256;
            let mut input = String::with_capacity(length * 2);
            for _ in 0..length {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                input.push(alphabet[(state as usize) % alphabet.len()]);
            }
            assert!(std::panic::catch_unwind(|| parse(&input)).is_ok());
            assert!(std::panic::catch_unwind(|| parse_expression(&input)).is_ok());
        }
    }

    #[test]
    fn deterministic_mutations_of_valid_modules_never_panic() {
        let seed = "mod fuzz\nfn echo(value: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let copy: i32 = value\n    if copy > 0:\n      return copy\n    else:\n      return 0\n";
        let alphabet = [
            ' ', '\n', '#', '(', ')', ':', ',', '-', '>', '=', '0', 'x', 'ع',
        ];
        let mut state = 0xabad1dea_u64;
        for iteration in 0..2_000 {
            let mut input = seed.as_bytes().to_vec();
            for _ in 0..1 + (iteration % 3) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let index = (state as usize) % input.len();
                let replacement = alphabet[(state.rotate_left(17) as usize) % alphabet.len()];
                input[index] = replacement as u8;
            }
            let input = String::from_utf8(input).expect("mutations preserve UTF-8 boundaries");
            assert!(std::panic::catch_unwind(|| parse(&input)).is_ok());
        }
    }
}
