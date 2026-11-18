//! Structural parser for the first IRQ subset.
//! Expression parsing is provided for the MVP expression subset.

use irq_ast::{BinaryOp, Expression, Function, Module, Parameter, Statement, UnaryOp};
use irq_lexer::{Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub code: &'static str,
    pub line: usize,
    pub message: String,
}

/// Parse one expression with explicit precedence and associativity.
pub fn parse_expression(source: &str) -> Result<Expression, ParseError> {
    let tokens = irq_lexer::lex(source).map_err(|e| ParseError {
        code: "E0002",
        line: e.span.line,
        message: e.message,
    })?;
    let mut p = ExprParser { tokens, pos: 0 };
    let expr = p.expression(0)?;
    if !matches!(p.peek(), TokenKind::Eof | TokenKind::Newline) {
        return Err(p.err("unexpected token after expression"));
    }
    Ok(expr)
}

struct ExprParser {
    tokens: Vec<Token>,
    pos: usize,
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
            code: "E0101",
            line: self.tokens[self.pos.min(self.tokens.len() - 1)].span.line,
            message: message.into(),
        }
    }
    fn expression(&mut self, min_bp: u8) -> Result<Expression, ParseError> {
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
            TokenKind::Ident(n) => Expression::Name(n),
            TokenKind::Minus => Expression::Unary {
                op: UnaryOp::Negate,
                value: Box::new(self.expression(11)?),
            },
            TokenKind::LParen => {
                let e = self.expression(0)?;
                if !matches!(self.take(), TokenKind::RParen) {
                    return Err(self.err("expected `)`"));
                }
                e
            }
            _ => return Err(self.err("expected expression")),
        };
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
                            self.take();
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
                _ => break,
            };
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
}

pub fn parse(source: &str) -> Result<Module, ParseError> {
    irq_lexer::lex(source).map_err(|e| ParseError {
        code: "E0002",
        line: e.span.line,
        message: e.message,
    })?;
    let rows: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .map(|(i, s)| (i + 1, s.split('#').next().unwrap_or("").trim_end()))
        .filter(|(_, s)| !s.trim().is_empty())
        .collect();
    let Some((mod_line, first)) = rows.first().copied() else {
        return Err(error(1, "file is empty; expected `mod name`"));
    };
    let mut parts = first.split_whitespace();
    if parts.next() != Some("mod") {
        return Err(error(mod_line, "file must start with `mod name`"));
    }
    let name = parts
        .next()
        .ok_or_else(|| error(mod_line, "module name is missing"))?;
    if !valid_ident(name) || parts.next().is_some() {
        return Err(error(mod_line, "invalid module declaration"));
    }

    let mut functions = Vec::new();
    let mut i = 1;
    while i < rows.len() {
        let (line, text) = rows[i];
        let trimmed = text.trim();
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
        let close = signature[open..]
            .find(')')
            .map(|n| n + open)
            .ok_or_else(|| error(line, "expected `)` in function signature"))?;
        if !signature[close..].contains("->") {
            return Err(error(
                line,
                "function signature must declare `-> ReturnType`",
            ));
        }
        let params_text = &signature[open + 1..close];
        let mut parameters = Vec::new();
        if !params_text.trim().is_empty() {
            for param in params_text.split(',') {
                let (param_name, param_ty) = param
                    .split_once(':')
                    .ok_or_else(|| error(line, "each parameter requires `name: Type`"))?;
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
        let return_tail = signature[close + 1..]
            .trim()
            .strip_prefix("->")
            .ok_or_else(|| error(line, "expected `-> ReturnType`"))?
            .trim();
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
            if let Some(key) = ["reads:", "writes:", "requires:", "ensures:", "body:"]
                .iter()
                .find(|k| section.starts_with(**k))
            {
                let key = key.trim_end_matches(':');
                if sections.iter().any(|(existing, _)| *existing == key) {
                    return Err(error(rows[i].0, &format!("duplicate `{key}:` section")));
                }
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
        let body = parse_block(&rows, &mut body_cursor, 4)?;
        if body.is_empty() {
            return Err(error(
                rows[body_index].0,
                "function body must contain at least one supported statement",
            ));
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

fn valid_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(x) if x.is_ascii_alphabetic() || x == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

fn parse_block(
    rows: &[(usize, &str)],
    cursor: &mut usize,
    expected_indent: usize,
) -> Result<Vec<Statement>, ParseError> {
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
            let condition = parse_expression(condition.trim()).map_err(|mut e| {
                e.line = line;
                e
            })?;
            *cursor += 1;
            let then_body = parse_block(rows, cursor, expected_indent + 2)?;
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
            let else_body = parse_block(rows, cursor, expected_indent + 2)?;
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
            let condition = parse_expression(condition.trim()).map_err(|mut e| {
                e.line = line;
                e
            })?;
            *cursor += 1;
            let body = parse_block(rows, cursor, expected_indent + 2)?;
            if body.is_empty() {
                return Err(error(line, "while body must not be empty"));
            }
            statements.push(Statement::While { condition, body });
        } else {
            statements.push(parse_statement(trimmed, line)?);
            *cursor += 1;
        }
    }
    Ok(statements)
}

fn parse_statement(source: &str, line: usize) -> Result<Statement, ParseError> {
    if let Some(expr) = source.strip_prefix("return ") {
        return Ok(Statement::Return(parse_expression(expr).map_err(
            |mut e| {
                e.line = line;
                e
            },
        )?));
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
        let value = parse_expression(rhs.trim()).map_err(|mut e| {
            e.line = line;
            e
        })?;
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
            let value = parse_expression(rhs.trim()).map_err(|mut e| {
                e.line = line;
                e
            })?;
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

fn error(line: usize, message: &str) -> ParseError {
    ParseError {
        code: "E0100",
        line,
        message: message.to_owned(),
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
    fn multiplication_binds_tighter_than_addition() {
        let parsed = parse_expression("1 + 2 * 3").unwrap();
        assert!(
            matches!(parsed, Expression::Binary { op: BinaryOp::Add, ref right, .. }
            if matches!(**right, Expression::Binary { op: BinaryOp::Multiply, .. }))
        );
    }

    #[test]
    fn rejects_incomplete_expression() {
        assert!(parse_expression("1 +").is_err());
        assert!(parse_expression("true false").is_err());
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
}
