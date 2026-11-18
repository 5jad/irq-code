//! Rust emitter for the checked IRQ subset. This is a temporary execution backend.

use irq_ast::{BinaryOp, Expression, Function, Module, Statement, UnaryOp};
use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenError(pub String);

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CodegenError {}

type Signatures = HashMap<String, (Vec<String>, String)>;
type Types = HashMap<String, String>;

/// Emits standalone Rust source for an already parsed and type-checked IRQ module.
pub fn emit(module: &Module) -> Result<String, CodegenError> {
    let signatures: Signatures = module
        .functions
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                (
                    f.parameters.iter().map(|p| p.ty.clone()).collect(),
                    f.return_type.clone(),
                ),
            )
        })
        .collect();
    let mut out = String::from(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum IrqRuntimeError { Overflow, DivisionByZero }\n\n",
    );
    for function in &module.functions {
        emit_function(function, &signatures, &mut out)?;
        out.push('\n');
    }
    Ok(out)
}

fn emit_function(
    f: &Function,
    signatures: &Signatures,
    out: &mut String,
) -> Result<(), CodegenError> {
    writeln!(out, "#[allow(dead_code, non_snake_case)]").unwrap();
    if f.signature.starts_with("pub fn ") {
        out.push_str("pub ");
    }
    write!(out, "fn __irq_{}(", f.name).unwrap();
    let mut env = Types::new();
    for (i, p) in f.parameters.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "__irq_{}: {}", p.name, rust_type(&p.ty)?).unwrap();
        env.insert(p.name.clone(), p.ty.clone());
    }
    writeln!(
        out,
        ") -> Result<{}, IrqRuntimeError> {{",
        rust_type(&f.return_type)?
    )
    .unwrap();
    emit_block(&f.body, 1, &mut env, &f.return_type, signatures, out)?;
    writeln!(out, "}}").unwrap();
    Ok(())
}

fn emit_block(
    body: &[Statement],
    depth: usize,
    env: &mut Types,
    return_type: &str,
    signatures: &Signatures,
    out: &mut String,
) -> Result<(), CodegenError> {
    let pad = "    ".repeat(depth);
    for stmt in body {
        match stmt {
            Statement::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let value = emit_expr(value, Some(ty), env, signatures)?;
                writeln!(
                    out,
                    "{pad}let {}__irq_{}: {} = {};",
                    if *mutable { "mut " } else { "" },
                    name,
                    rust_type(ty)?,
                    value
                )
                .unwrap();
                env.insert(name.clone(), ty.clone());
            }
            Statement::Assign { name, value } => {
                let ty = env
                    .get(name)
                    .ok_or_else(|| CodegenError(format!("missing type for `{name}`")))?;
                let value = emit_expr(value, Some(ty), env, signatures)?;
                writeln!(out, "{pad}__irq_{} = {};", name, value).unwrap();
            }
            Statement::Return(value) => {
                let value = emit_expr(value, Some(return_type), env, signatures)?;
                writeln!(out, "{pad}return Ok({value});").unwrap();
            }
            Statement::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = emit_expr(condition, Some("bool"), env, signatures)?;
                writeln!(out, "{pad}if {condition} {{").unwrap();
                let mut then_env = env.clone();
                emit_block(
                    then_body,
                    depth + 1,
                    &mut then_env,
                    return_type,
                    signatures,
                    out,
                )?;
                writeln!(out, "{pad}}} else {{").unwrap();
                let mut else_env = env.clone();
                emit_block(
                    else_body,
                    depth + 1,
                    &mut else_env,
                    return_type,
                    signatures,
                    out,
                )?;
                writeln!(out, "{pad}}}").unwrap();
            }
            Statement::While { condition, body } => {
                let condition = emit_expr(condition, Some("bool"), env, signatures)?;
                writeln!(out, "{pad}while {condition} {{").unwrap();
                let mut loop_env = env.clone();
                emit_block(body, depth + 1, &mut loop_env, return_type, signatures, out)?;
                writeln!(out, "{pad}}}").unwrap();
            }
        }
    }
    Ok(())
}

fn emit_expr(
    expr: &Expression,
    hint: Option<&str>,
    env: &Types,
    signatures: &Signatures,
) -> Result<String, CodegenError> {
    match expr {
        Expression::Integer(n) => {
            let ty = hint.unwrap_or("i32");
            Ok(format!("{n}{ty}"))
        }
        Expression::Boolean(v) => Ok(v.to_string()),
        Expression::String(v) => Ok(format!("String::from({v:?})")),
        Expression::Name(name) => {
            if env.get(name).is_some_and(|ty| ty == "str") {
                Ok(format!("__irq_{name}.clone()"))
            } else {
                Ok(format!("__irq_{name}"))
            }
        }
        Expression::Call { callee, arguments } => {
            let (params, _) = signatures
                .get(callee)
                .ok_or_else(|| CodegenError(format!("unknown function `{callee}`")))?;
            let args = arguments
                .iter()
                .zip(params)
                .map(|(arg, ty)| emit_expr(arg, Some(ty), env, signatures))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("__irq_{callee}({})?", args.join(", ")))
        }
        Expression::Unary {
            op: UnaryOp::Not,
            value,
        } => Ok(format!(
            "!({})",
            emit_expr(value, Some("bool"), env, signatures)?
        )),
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => {
            let ty = hint.unwrap_or("i32");
            if let Expression::Integer(magnitude) = value.as_ref() {
                if ty == "i32" && *magnitude == 2_147_483_648 {
                    return Ok("i32::MIN".into());
                }
                if ty == "i64" && *magnitude == 9_223_372_036_854_775_808 {
                    return Ok("i64::MIN".into());
                }
            }
            let inner = emit_expr(value, Some(ty), env, signatures)?;
            Ok(format!(
                "{{ let __irq_value: {} = {inner}; __irq_value.checked_neg().ok_or(IrqRuntimeError::Overflow)? }}",
                rust_type(ty)?
            ))
        }
        Expression::Binary { left, op, right } => {
            emit_binary(left, *op, right, hint, env, signatures)
        }
    }
}

fn emit_binary(
    left: &Expression,
    op: BinaryOp,
    right: &Expression,
    hint: Option<&str>,
    env: &Types,
    signatures: &Signatures,
) -> Result<String, CodegenError> {
    if matches!(
        op,
        BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
    ) {
        let ty = numeric_operand_type(left, right, hint, env, signatures)?;
        let l = emit_expr(left, Some(&ty), env, signatures)?;
        let r = emit_expr(right, Some(&ty), env, signatures)?;
        let method = match op {
            BinaryOp::Add => "checked_add",
            BinaryOp::Subtract => "checked_sub",
            BinaryOp::Multiply => "checked_mul",
            BinaryOp::Divide => "checked_div",
            _ => unreachable!(),
        };
        let zero_check = if op == BinaryOp::Divide {
            "if __irq_r == 0 { return Err(IrqRuntimeError::DivisionByZero); } "
        } else {
            ""
        };
        return Ok(format!(
            "{{ let __irq_l: {} = {l}; let __irq_r: {} = {r}; {zero_check}__irq_l.{method}(__irq_r).ok_or(IrqRuntimeError::Overflow)? }}",
            rust_type(&ty)?,
            rust_type(&ty)?
        ));
    }
    let common_hint = if matches!(op, BinaryOp::And | BinaryOp::Or) {
        "bool".to_owned()
    } else if is_constant_integer(left) {
        expression_type(right, env, signatures)?
    } else {
        expression_type(left, env, signatures)?
    };
    let l = emit_expr(left, Some(&common_hint), env, signatures)?;
    let r = emit_expr(right, Some(&common_hint), env, signatures)?;
    if common_hint == "str" && matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) {
        let symbol = if op == BinaryOp::Equal { "==" } else { "!=" };
        return Ok(format!("&({l}) {symbol} &({r})"));
    }
    let symbol = match op {
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        _ => return Err(CodegenError("invalid non-arithmetic operator".into())),
    };
    Ok(format!("({l}) {symbol} ({r})"))
}

fn numeric_operand_type(
    left: &Expression,
    right: &Expression,
    hint: Option<&str>,
    env: &Types,
    signatures: &Signatures,
) -> Result<String, CodegenError> {
    if let Some(hint) = hint {
        return Ok(hint.to_owned());
    }
    if !is_constant_integer(left) {
        return expression_type(left, env, signatures);
    }
    if !is_constant_integer(right) {
        return expression_type(right, env, signatures);
    }
    Ok("i32".into())
}

fn expression_type(
    expr: &Expression,
    env: &Types,
    signatures: &Signatures,
) -> Result<String, CodegenError> {
    match expr {
        Expression::Integer(_) => Ok("i32".into()),
        Expression::Boolean(_) => Ok("bool".into()),
        Expression::String(_) => Ok("str".into()),
        Expression::Name(n) => env
            .get(n)
            .cloned()
            .ok_or_else(|| CodegenError(format!("unknown name `{n}`"))),
        Expression::Call { callee, .. } => signatures
            .get(callee)
            .map(|(_, ret)| ret.clone())
            .ok_or_else(|| CodegenError(format!("unknown function `{callee}`"))),
        Expression::Unary {
            op: UnaryOp::Not, ..
        } => Ok("bool".into()),
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => expression_type(value, env, signatures),
        Expression::Binary {
            op: BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide,
            left,
            right,
        } => numeric_operand_type(left, right, None, env, signatures),
        Expression::Binary { .. } => Ok("bool".into()),
    }
}

fn is_constant_integer(expr: &Expression) -> bool {
    match expr {
        Expression::Integer(_) => true,
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => is_constant_integer(value),
        Expression::Binary { left, op, right } => {
            matches!(
                op,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
            ) && is_constant_integer(left)
                && is_constant_integer(right)
        }
        _ => false,
    }
}

fn rust_type(ty: &str) -> Result<&'static str, CodegenError> {
    match ty {
        "i32" => Ok("i32"),
        "i64" => Ok("i64"),
        "u32" => Ok("u32"),
        "u64" => Ok("u64"),
        "bool" => Ok("bool"),
        "str" => Ok("String"),
        _ => Err(CodegenError(format!(
            "unsupported Rust backend type `{ty}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn generated_rust_compiles_and_enforces_checked_arithmetic() {
        let irq = "mod backend\nfn add(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a + b\nfn subtract(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a - b\nfn multiply(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a * b\nfn negate(a: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return -a\nfn add_u64(a: u64, b: u64) -> u64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a + b\nfn divide(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a / b\nfn min_i64() -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return -9223372036854775808\nfn echo(value: str) -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return value\nfn greeting() -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return echo(\"مرحبا\")\nfn compare_return(value: str) -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if value == \"مرحبا\":\n      return value\n    else:\n      return value\n";
        let irq = format!(
            "{irq}fn bool_expr(a: bool, b: bool, c: bool) -> bool\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return (a or b) and c\n"
        );
        let irq = format!(
            "{irq}fn count_to_three() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut n: i32 = 0\n    while n < 3:\n      n = n + 1\n    return n\n"
        );
        let module = irq_parser::parse(&irq).unwrap();
        irq_check::check(&module).unwrap();
        let generated = emit(&module).unwrap();
        let test_harness = r#"
#[cfg(test)]
mod generated_tests {
    use super::*;
    #[test] fn ordinary_addition() { assert_eq!(__irq_add(2, 3), Ok(5)); }
    #[test] fn addition_overflow_is_returned() { assert_eq!(__irq_add(i32::MAX, 1), Err(IrqRuntimeError::Overflow)); }
    #[test] fn subtraction_overflow_is_returned() { assert_eq!(__irq_subtract(i32::MIN, 1), Err(IrqRuntimeError::Overflow)); }
    #[test] fn multiplication_overflow_is_returned() { assert_eq!(__irq_multiply(i32::MAX, 2), Err(IrqRuntimeError::Overflow)); }
    #[test] fn negation_overflow_is_returned() { assert_eq!(__irq_negate(i32::MIN), Err(IrqRuntimeError::Overflow)); }
    #[test] fn unsigned_overflow_is_returned() { assert_eq!(__irq_add_u64(u64::MAX, 1), Err(IrqRuntimeError::Overflow)); }
    #[test] fn division_by_zero_is_returned() { assert_eq!(__irq_divide(8, 0), Err(IrqRuntimeError::DivisionByZero)); }
    #[test] fn signed_division_overflow_is_returned() { assert_eq!(__irq_divide(i32::MIN, -1), Err(IrqRuntimeError::Overflow)); }
    #[test] fn signed_minimum_literal_is_preserved() { assert_eq!(__irq_min_i64(), Ok(i64::MIN)); }
    #[test] fn unicode_string_round_trips() { assert_eq!(__irq_greeting(), Ok(String::from("مرحبا"))); }
    #[test] fn string_comparison_does_not_consume_value() { assert_eq!(__irq_compare_return(String::from("مرحبا")), Ok(String::from("مرحبا"))); }
    #[test] fn boolean_precedence_matches_irq_ast() { assert_eq!(__irq_bool_expr(true, false, false), Ok(false)); }
    #[test] fn while_rechecks_condition_and_mutates_outer_local() { assert_eq!(__irq_count_to_three(), Ok(3)); }
}
"#;
        let id = std::process::id();
        let base = std::env::temp_dir().join(format!("irq_codegen_{id}"));
        let source_path = base.with_extension("rs");
        let binary_path = base.with_extension("bin");
        fs::write(&source_path, format!("{generated}\n{test_harness}")).unwrap();
        let compile = Command::new("rustc")
            .arg("--test")
            .arg("-D")
            .arg("warnings")
            .arg(&source_path)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .expect("rustc must be available when running Rust workspace tests");
        assert!(
            compile.status.success(),
            "rustc failed:\n{}\n--- generated source ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            generated
        );
        let run = Command::new(&binary_path).output().unwrap();
        assert!(
            run.status.success(),
            "generated backend tests failed:\n{}",
            String::from_utf8_lossy(&run.stdout)
        );
        let _ = fs::remove_file(source_path);
        let _ = fs::remove_file(binary_path);
    }
}
