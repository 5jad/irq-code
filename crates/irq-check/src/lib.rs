//! First scalar type checker for the executable IRQ subset.

use irq_ast::{BinaryOp, Expression, Function, Module, Statement, UnaryOp};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckError {
    pub code: &'static str,
    pub function: String,
    pub message: String,
}
type Env = HashMap<String, (String, bool)>;

pub fn check(module: &Module) -> Result<(), Vec<CheckError>> {
    let mut errors = Vec::new();
    let mut signatures: HashMap<String, (Vec<String>, String)> = HashMap::new();
    for f in &module.functions {
        let signature = (
            f.parameters.iter().map(|p| p.ty.clone()).collect(),
            f.return_type.clone(),
        );
        if signatures.insert(f.name.clone(), signature).is_some() {
            errors.push(err("E0208", f, format!("duplicate function `{}`", f.name)));
        }
    }
    for f in &module.functions {
        if !supported_type(&f.return_type) {
            errors.push(err(
                "E0200",
                f,
                format!("unsupported return type `{}`", f.return_type),
            ));
            continue;
        }
        let mut env: Env = HashMap::new();
        for p in &f.parameters {
            if !supported_type(&p.ty) {
                errors.push(err(
                    "E0200",
                    f,
                    format!("unsupported parameter type `{}`", p.ty),
                ));
            }
            if env.insert(p.name.clone(), (p.ty.clone(), false)).is_some() {
                errors.push(err("E0201", f, format!("duplicate name `{}`", p.name)));
            }
        }
        let mut returned = false;
        for (index, stmt) in f.body.iter().enumerate() {
            if returned {
                errors.push(err("E0202", f, "unreachable statement after return".into()));
                break;
            }
            match stmt {
                Statement::Let {
                    name,
                    mutable,
                    ty,
                    value,
                } => {
                    if !supported_type(ty) {
                        errors.push(err("E0200", f, format!("unsupported local type `{ty}`")));
                        continue;
                    }
                    if env.contains_key(name) {
                        errors.push(err(
                            "E0201",
                            f,
                            format!("name `{name}` is already defined in this function"),
                        ));
                        continue;
                    }
                    match infer_expected(value, ty, &env, &signatures) {
                        Ok(actual) if actual == *ty => {}
                        Ok(actual) => errors.push(err(
                            "E0203",
                            f,
                            format!(
                                "type mismatch for `{name}`: expected `{ty}`, found `{actual}`"
                            ),
                        )),
                        Err(e) => errors.push(err("E0204", f, e)),
                    }
                    // Preserve the declared binding after a primary initializer error
                    // to avoid cascading `unknown name` diagnostics.
                    env.insert(name.clone(), (ty.clone(), *mutable));
                }
                Statement::Assign { name, value } => match env.get(name) {
                    None => errors.push(err("E0205", f, format!("unknown variable `{name}`"))),
                    Some((_, false)) => errors.push(err(
                        "E0206",
                        f,
                        format!("cannot assign to immutable variable `{name}`"),
                    )),
                    Some((expected, true)) => {
                        match infer_expected(value, expected, &env, &signatures) {
                            Ok(actual) if actual == *expected => {}
                            Ok(actual) => errors.push(err(
                                "E0203",
                                f,
                                format!(
                                    "assignment to `{name}` expects `{expected}`, found `{actual}`"
                                ),
                            )),
                            Err(e) => errors.push(err("E0204", f, e)),
                        }
                    }
                },
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    match infer(condition, &env, &signatures) {
                        Ok(t) if t == "bool" => {}
                        Ok(t) => errors.push(err(
                            "E0209",
                            f,
                            format!("if condition must be bool, found `{t}`"),
                        )),
                        Err(e) => errors.push(err("E0204", f, e)),
                    }
                    let then_returns =
                        check_return_branch(then_body, f, &env, &signatures, &mut errors);
                    let else_returns =
                        check_return_branch(else_body, f, &env, &signatures, &mut errors);
                    // If both branches return, the function is complete here. If one
                    // or both continue, subsequent statements are checked on fallthrough.
                    returned = then_returns && else_returns;
                    if returned && index + 1 != f.body.len() {
                        errors.push(err(
                            "E0202",
                            f,
                            "if/else return must be the final statement".into(),
                        ));
                        break;
                    }
                }
                Statement::Return(value) => {
                    match infer_expected(value, &f.return_type, &env, &signatures) {
                        Ok(actual) if actual == f.return_type => {}
                        Ok(actual) => errors.push(err(
                            "E0203",
                            f,
                            format!("return expects `{}`, found `{actual}`", f.return_type),
                        )),
                        Err(e) => errors.push(err("E0204", f, e)),
                    }
                    returned = true;
                    if index + 1 != f.body.len() {
                        errors.push(err(
                            "E0202",
                            f,
                            "return must be the final statement in this MVP subset".into(),
                        ));
                        break;
                    }
                }
            }
        }
        if !returned {
            errors.push(err(
                "E0207",
                f,
                "function has no final return statement".into(),
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn supported_type(ty: &str) -> bool {
    matches!(ty, "i32" | "i64" | "u32" | "u64" | "bool" | "str")
}

fn check_return_branch(
    body: &[Statement],
    function: &Function,
    env: &Env,
    signatures: &HashMap<String, (Vec<String>, String)>,
    errors: &mut Vec<CheckError>,
) -> bool {
    let mut branch_env = env.clone();
    for (index, stmt) in body.iter().enumerate() {
        let is_last = index + 1 == body.len();
        match stmt {
            Statement::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                if !supported_type(ty) {
                    errors.push(err(
                        "E0200",
                        function,
                        format!("unsupported local type `{ty}` in branch"),
                    ));
                    continue;
                }
                if branch_env.contains_key(name) {
                    errors.push(err(
                        "E0201",
                        function,
                        format!("name `{name}` is already defined in this branch"),
                    ));
                    continue;
                }
                match infer_expected(value, ty, &branch_env, signatures) {
                    Ok(actual) if actual == *ty => {}
                    Ok(actual) => errors.push(err(
                        "E0203",
                        function,
                        format!("branch variable `{name}` expects `{ty}`, found `{actual}`"),
                    )),
                    Err(e) => errors.push(err("E0204", function, e)),
                }
                branch_env.insert(name.clone(), (ty.clone(), *mutable));
            }
            Statement::Assign { name, value } => match branch_env.get(name) {
                None => errors.push(err(
                    "E0205",
                    function,
                    format!("unknown variable `{name}` in branch"),
                )),
                Some((_, false)) => errors.push(err(
                    "E0206",
                    function,
                    format!("cannot assign to immutable variable `{name}` in branch"),
                )),
                Some((expected, true)) => match infer_expected(
                    value,
                    expected,
                    &branch_env,
                    signatures,
                ) {
                    Ok(actual) if actual == *expected => {}
                    Ok(actual) => errors.push(err(
                        "E0203",
                        function,
                        format!(
                            "branch assignment to `{name}` expects `{expected}`, found `{actual}`"
                        ),
                    )),
                    Err(e) => errors.push(err("E0204", function, e)),
                },
            },
            Statement::If {
                condition,
                then_body,
                else_body,
            } => {
                match infer(condition, &branch_env, signatures) {
                    Ok(t) if t == "bool" => {}
                    Ok(t) => errors.push(err(
                        "E0209",
                        function,
                        format!("nested if condition must be bool, found `{t}`"),
                    )),
                    Err(e) => errors.push(err("E0204", function, e)),
                }
                let then_returns =
                    check_return_branch(then_body, function, &branch_env, signatures, errors);
                let else_returns =
                    check_return_branch(else_body, function, &branch_env, signatures, errors);
                if then_returns && else_returns && !is_last {
                    errors.push(err(
                        "E0202",
                        function,
                        "unreachable statement after a returning nested if".into(),
                    ));
                }
                if then_returns && else_returns {
                    return true;
                }
            }
            Statement::Return(value) => {
                if !is_last {
                    errors.push(err(
                        "E0202",
                        function,
                        "return must be the final statement in a branch".into(),
                    ));
                }
                match infer_expected(value, &function.return_type, &branch_env, signatures) {
                    Ok(actual) if actual == function.return_type => {}
                    Ok(actual) => errors.push(err(
                        "E0203",
                        function,
                        format!(
                            "branch return expects `{}`, found `{actual}`",
                            function.return_type
                        ),
                    )),
                    Err(e) => errors.push(err("E0204", function, e)),
                }
                return true;
            }
        }
    }
    false
}

fn integer_type(ty: &str) -> bool {
    matches!(ty, "i32" | "i64" | "u32" | "u64")
}
fn err(code: &'static str, f: &Function, message: String) -> CheckError {
    CheckError {
        code,
        function: f.name.clone(),
        message,
    }
}

fn infer(
    expr: &Expression,
    env: &Env,
    signatures: &HashMap<String, (Vec<String>, String)>,
) -> Result<String, String> {
    match expr {
        Expression::Integer(n) => {
            if fits_integer(*n, "i32") {
                Ok("i32".into())
            } else {
                Err(format!(
                    "integer literal `{n}` does not fit the default type `i32`"
                ))
            }
        }
        Expression::Boolean(_) => Ok("bool".into()),
        Expression::String(_) => Ok("str".into()),
        Expression::Name(n) => env
            .get(n)
            .map(|(t, _)| t.clone())
            .ok_or_else(|| format!("unknown name `{n}`")),
        Expression::Call { callee, arguments } => {
            let (params, result) = signatures
                .get(callee)
                .ok_or_else(|| format!("unknown function `{callee}`"))?;
            if params.len() != arguments.len() {
                return Err(format!(
                    "function `{callee}` expects {} arguments, found {}",
                    params.len(),
                    arguments.len()
                ));
            }
            for (index, (argument, expected)) in arguments.iter().zip(params).enumerate() {
                let actual = infer_expected(argument, expected, env, signatures)?;
                if actual != *expected {
                    return Err(format!(
                        "argument {} of `{callee}` expects `{expected}`, found `{actual}`",
                        index + 1
                    ));
                }
            }
            Ok(result.clone())
        }
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => {
            if is_const_integer(expr) {
                let result =
                    const_int(expr).ok_or_else(|| "compile-time integer overflow".to_owned())?;
                if fits_integer(result, "i32") {
                    return Ok("i32".into());
                }
                return Err("compile-time integer overflow for default type `i32`".into());
            }
            let t = infer(value, env, signatures)?;
            if matches!(t.as_str(), "i32" | "i64") {
                Ok(t)
            } else {
                Err(format!("unary `-` requires a signed integer, found `{t}`"))
            }
        }
        Expression::Unary {
            op: UnaryOp::Not,
            value,
        } => {
            let t = infer(value, env, signatures)?;
            if t == "bool" {
                Ok(t)
            } else {
                Err(format!("`not` requires bool, found `{t}`"))
            }
        }
        Expression::Binary { left, op, right } => {
            let (l, r) = infer_numeric_pair(left, right, env, signatures)?;
            if *op == BinaryOp::Divide && const_int(right) == Some(0) {
                return Err("division by a compile-time zero divisor".into());
            }
            match op {
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                    if l == r && integer_type(&l) {
                        if is_const_integer(expr) {
                            let value = const_int(expr)
                                .ok_or_else(|| "compile-time integer overflow".to_owned())?;
                            if !fits_integer(value, &l) {
                                return Err(format!(
                                    "compile-time result is outside the range of `{l}`"
                                ));
                            }
                        }
                        Ok(l)
                    } else {
                        Err(format!(
                            "arithmetic operands must have the same integer type, found `{l}` and `{r}"
                        ))
                    }
                }
                BinaryOp::And | BinaryOp::Or => {
                    if l == "bool" && r == "bool" {
                        Ok("bool".into())
                    } else {
                        Err(format!(
                            "logical operators require bool operands, found `{l}` and `{r}`"
                        ))
                    }
                }
                BinaryOp::Equal | BinaryOp::NotEqual => {
                    if l == r {
                        Ok("bool".into())
                    } else {
                        Err(format!(
                            "equality operands have different types `{l}` and `{r}`"
                        ))
                    }
                }
                BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => {
                    if l == r && integer_type(&l) {
                        Ok("bool".into())
                    } else {
                        Err(format!(
                            "ordered comparison requires matching integer types, found `{l}` and `{r}`"
                        ))
                    }
                }
            }
        }
    }
}

fn infer_numeric_pair(
    left: &Expression,
    right: &Expression,
    env: &Env,
    signatures: &HashMap<String, (Vec<String>, String)>,
) -> Result<(String, String), String> {
    let left_result = infer(left, env, signatures);
    let right_result = infer(right, env, signatures);
    match (left_result, right_result) {
        (Ok(l), Ok(r)) if l == r => Ok((l, r)),
        (Ok(l), Ok(_r)) if integer_type(&l) && is_const_integer(right) => {
            let r = infer_expected(right, &l, env, signatures)?;
            Ok((l, r))
        }
        (Ok(_l), Ok(r)) if integer_type(&r) && is_const_integer(left) => {
            let l = infer_expected(left, &r, env, signatures)?;
            Ok((l, r))
        }
        (Err(_), Ok(r)) if integer_type(&r) && is_const_integer(left) => {
            let l = infer_expected(left, &r, env, signatures)?;
            Ok((l, r))
        }
        (Ok(l), Err(_)) if integer_type(&l) && is_const_integer(right) => {
            let r = infer_expected(right, &l, env, signatures)?;
            Ok((l, r))
        }
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e),
        (Ok(l), Ok(r)) => Ok((l, r)),
    }
}

fn const_int(expr: &Expression) -> Option<i128> {
    match expr {
        Expression::Integer(n) => Some(*n),
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => const_int(value)?.checked_neg(),
        Expression::Binary { left, op, right } => {
            let l = const_int(left)?;
            let r = const_int(right)?;
            match op {
                BinaryOp::Add => l.checked_add(r),
                BinaryOp::Subtract => l.checked_sub(r),
                BinaryOp::Multiply => l.checked_mul(r),
                BinaryOp::Divide => l.checked_div(r),
                _ => None,
            }
        }
        _ => None,
    }
}

fn infer_expected(
    expr: &Expression,
    expected: &str,
    env: &Env,
    signatures: &HashMap<String, (Vec<String>, String)>,
) -> Result<String, String> {
    if integer_type(expected) && is_const_integer(expr) {
        if has_const_zero_divisor(expr) {
            return Err("division by a compile-time zero divisor".into());
        }
        let value = const_int(expr).ok_or_else(|| "compile-time integer overflow".to_owned())?;
        if !fits_integer(value, expected) {
            return Err(format!(
                "constant value `{value}` is outside the range of `{expected}`"
            ));
        }
        return Ok(expected.to_owned());
    }
    infer(expr, env, signatures)
}

fn fits_integer(value: i128, ty: &str) -> bool {
    match ty {
        "i32" => i32::try_from(value).is_ok(),
        "i64" => i64::try_from(value).is_ok(),
        "u32" => u32::try_from(value).is_ok(),
        "u64" => u64::try_from(value).is_ok(),
        _ => false,
    }
}

fn is_const_integer(expr: &Expression) -> bool {
    match expr {
        Expression::Integer(_) => true,
        Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => is_const_integer(value),
        Expression::Binary { left, op, right } => {
            matches!(
                op,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
            ) && is_const_integer(left)
                && is_const_integer(right)
        }
        _ => false,
    }
}

fn has_const_zero_divisor(expr: &Expression) -> bool {
    match expr {
        Expression::Binary { left, op, right } => {
            (*op == BinaryOp::Divide && const_int(right) == Some(0))
                || has_const_zero_divisor(left)
                || has_const_zero_divisor(right)
        }
        Expression::Unary { value, .. } => has_const_zero_divisor(value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use irq_parser::parse;
    fn program(body: &str) -> Module {
        parse(&format!("mod test\nfn calc(a: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n{body}\n")).unwrap()
    }
    #[test]
    fn accepts_typed_mutable_local_and_return() {
        assert!(
            check(&program(
                "    let mut x: i32 = a + 2\n    x = x * 3\n    return x"
            ))
            .is_ok()
        );
    }
    #[test]
    fn rejects_assignment_to_immutable_local() {
        assert!(
            check(&program("    let x: i32 = a\n    x = 3\n    return x"))
                .unwrap_err()
                .iter()
                .any(|e| e.code == "E0206")
        );
    }
    #[test]
    fn rejects_wrong_return_type() {
        assert!(
            check(&program("    return true"))
                .unwrap_err()
                .iter()
                .any(|e| e.code == "E0203")
        );
    }

    #[test]
    fn rejects_compile_time_zero_divisors() {
        let direct = check(&program("    return 1 / 0")).unwrap_err();
        assert!(
            direct
                .iter()
                .any(|e| e.message.contains("compile-time zero divisor"))
        );
        let computed = check(&program("    return 8 / (3 - 3)")).unwrap_err();
        assert!(
            computed
                .iter()
                .any(|e| e.message.contains("compile-time zero divisor"))
        );
    }

    #[test]
    fn applies_expected_integer_type_to_constant_expressions() {
        let source = "mod numbers\nfn wide() -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 2147483648\nfn unsigned() -> u32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 4294967295\nfn sum() -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 2147483647 + 1\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn coerces_integer_literals_to_the_other_operand_type() {
        let source = "mod numbers\nfn add_right(a: i64) -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a + 1\nfn add_left(a: i64) -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 1 + a\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn rejects_integer_literals_and_constant_results_outside_target_range() {
        let literal = check(&program("    return 2147483648")).unwrap_err();
        assert!(
            literal
                .iter()
                .any(|e| e.message.contains("outside the range of `i32`"))
        );
        let overflow = check(&program("    return 2147483647 + 1")).unwrap_err();
        assert!(
            overflow
                .iter()
                .any(|e| e.message.contains("outside the range of `i32`"))
        );
    }

    #[test]
    fn accepts_signed_minima_and_unsigned_maxima() {
        let source = "mod boundaries\nfn min32() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return -2147483648\nfn min64() -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return -9223372036854775808\nfn max32() -> u32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 4294967295\nfn max64() -> u64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 18446744073709551615\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn rejects_values_immediately_outside_integer_boundaries() {
        let i32_below = "mod bad\nfn f() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return -2147483649\n";
        assert!(check(&parse(i32_below).unwrap()).is_err());
        let u64_above = "mod bad\nfn f() -> u64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 18446744073709551616\n";
        assert!(check(&parse(u64_above).unwrap()).is_err());
    }

    #[test]
    fn resolves_local_function_and_checks_arguments() {
        let source = "mod demo\nfn add(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a + b\nfn main() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return add(1, 2)\n";
        let module = parse(source).unwrap();
        assert!(check(&module).is_ok());
    }

    #[test]
    fn rejects_unknown_and_mistyped_calls() {
        let unknown = program("    return ghost(1)");
        assert!(
            check(&unknown).unwrap_err()[0]
                .message
                .contains("unknown function")
        );
        let source = "mod demo\nfn add(a: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a\nfn main() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return add(true)\n";
        let module = parse(source).unwrap();
        assert!(
            check(&module)
                .unwrap_err()
                .iter()
                .any(|e| e.message.contains("expects `i32`, found `bool`"))
        );
    }

    #[test]
    fn checks_if_else_condition_and_return_types() {
        let source = "mod demo\nfn sign(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if x > 0:\n      return 1\n    else:\n      return 0\n";
        assert!(check(&parse(source).unwrap()).is_ok());
        let bad = source.replace("if x > 0:", "if x:");
        assert!(
            check(&parse(&bad).unwrap())
                .unwrap_err()
                .iter()
                .any(|e| e.code == "E0209")
        );
    }

    #[test]
    fn checks_branch_local_variables_in_isolated_scopes() {
        let source = "mod demo\nfn sign(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if x > 0:\n      let y: i32 = x + 1\n      return y\n    else:\n      let y: i32 = 0\n      return y\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn checks_nested_if_else_paths() {
        let source = "mod demo\nfn classify(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if x > 0:\n      if x > 10:\n        return 10\n      else:\n        return 1\n    else:\n      return 0\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn accepts_if_else_fallthrough_with_final_return() {
        let source = "mod demo\nfn choose(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut y: i32 = 0\n    if x > 0:\n      y = 1\n    else:\n      y = 2\n    return y\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn accepts_mixed_return_and_fallthrough_when_fallthrough_returns_later() {
        let source = "mod demo\nfn choose(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut y: i32 = 0\n    if x > 0:\n      return 1\n    else:\n      y = 2\n    return y\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }

    #[test]
    fn rejects_if_with_a_fallthrough_path_that_never_returns() {
        let source = "mod demo\nfn choose(x: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut y: i32 = 0\n    if x > 0:\n      return 1\n    else:\n      y = 2\n";
        assert!(
            check(&parse(source).unwrap())
                .unwrap_err()
                .iter()
                .any(|e| e.code == "E0207")
        );
    }

    #[test]
    fn checks_string_literals_and_string_function_arguments() {
        let source = "mod strings\nfn echo(value: str) -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return value\nfn greeting() -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return echo(\"مرحبا\")\n";
        assert!(check(&parse(source).unwrap()).is_ok());
    }
}
