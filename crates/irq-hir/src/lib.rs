//! A normalized, typed intermediate representation for the checked IRQ subset.
//!
//! This initial HIR deliberately models values and control flow without references.
//! It is a compiler boundary, not yet an ownership proof or a memory-safety claim.

use irq_ast::{self as ast, BinaryOp, UnaryOp};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Unit,
    I32,
    I64,
    U32,
    U64,
    Bool,
    Str,
}

impl Type {
    fn parse(name: &str) -> Result<Self, LowerError> {
        match name {
            "()" => Ok(Self::Unit),
            "i32" => Ok(Self::I32),
            "i64" => Ok(Self::I64),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            "bool" => Ok(Self::Bool),
            "str" => Ok(Self::Str),
            other => Err(LowerError(format!("unsupported HIR type `{other}`"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Let {
        name: String,
        mutable: bool,
        ty: Type,
        value: Expression,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Return(Expression),
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub ty: Type,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    Unit,
    Integer(i128),
    Boolean(bool),
    String(String),
    Local(String),
    Call {
        callee: String,
        arguments: Vec<Expression>,
    },
    Unary {
        op: UnaryOp,
        value: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError(pub String);

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for LowerError {}

type Locals = HashMap<String, (Type, bool)>;
type Signatures = HashMap<String, (Vec<Type>, Type)>;

/// Lowers a successfully type-checked AST module into normalized typed HIR.
pub fn lower(module: &ast::Module) -> Result<Module, LowerError> {
    let signatures: Signatures = module
        .functions
        .iter()
        .map(|f| {
            Ok((
                f.name.clone(),
                (
                    f.parameters
                        .iter()
                        .map(|p| Type::parse(&p.ty))
                        .collect::<Result<_, _>>()?,
                    Type::parse(&f.return_type)?,
                ),
            ))
        })
        .collect::<Result<_, LowerError>>()?;
    let functions = module
        .functions
        .iter()
        .map(|f| {
            let parameters = f
                .parameters
                .iter()
                .map(|p| Ok((p.name.clone(), Type::parse(&p.ty)?)))
                .collect::<Result<Vec<_>, LowerError>>()?;
            let mut locals = parameters
                .iter()
                .map(|(name, ty)| (name.clone(), (ty.clone(), false)))
                .collect();
            let return_type = Type::parse(&f.return_type)?;
            let body = lower_block(&f.body, &mut locals, &signatures, &return_type)?;
            Ok(Function {
                name: f.name.clone(),
                parameters,
                return_type,
                body,
            })
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    Ok(Module {
        name: module.name.clone(),
        functions,
    })
}

fn lower_block(
    body: &[ast::Statement],
    locals: &mut Locals,
    signatures: &Signatures,
    return_type: &Type,
) -> Result<Vec<Statement>, LowerError> {
    body.iter()
        .map(|statement| match statement {
            ast::Statement::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let ty = Type::parse(ty)?;
                let value = lower_expr(value, Some(&ty), locals, signatures)?;
                locals.insert(name.clone(), (ty.clone(), *mutable));
                Ok(Statement::Let {
                    name: name.clone(),
                    mutable: *mutable,
                    ty,
                    value,
                })
            }
            ast::Statement::Assign { name, value } => {
                let ty = locals
                    .get(name)
                    .ok_or_else(|| LowerError(format!("unknown local `{name}`")))?
                    .0
                    .clone();
                Ok(Statement::Assign {
                    name: name.clone(),
                    value: lower_expr(value, Some(&ty), locals, signatures)?,
                })
            }
            ast::Statement::Return(value) => Ok(Statement::Return(lower_expr(
                value,
                Some(return_type),
                locals,
                signatures,
            )?)),
            ast::Statement::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = lower_expr(condition, Some(&Type::Bool), locals, signatures)?;
                let mut then_locals = locals.clone();
                let mut else_locals = locals.clone();
                Ok(Statement::If {
                    condition,
                    then_body: lower_block(then_body, &mut then_locals, signatures, return_type)?,
                    else_body: lower_block(else_body, &mut else_locals, signatures, return_type)?,
                })
            }
            ast::Statement::While { condition, body } => {
                let condition = lower_expr(condition, Some(&Type::Bool), locals, signatures)?;
                let mut loop_locals = locals.clone();
                Ok(Statement::While {
                    condition,
                    body: lower_block(body, &mut loop_locals, signatures, return_type)?,
                })
            }
        })
        .collect()
}

fn lower_expr(
    expr: &ast::Expression,
    hint: Option<&Type>,
    locals: &Locals,
    signatures: &Signatures,
) -> Result<Expression, LowerError> {
    use ast::Expression as E;
    let (ty, kind) = match expr {
        E::Unit => (Type::Unit, ExpressionKind::Unit),
        E::Integer(value) => (
            hint.cloned().unwrap_or(Type::I32),
            ExpressionKind::Integer(*value),
        ),
        E::Boolean(value) => (Type::Bool, ExpressionKind::Boolean(*value)),
        E::String(value) => (Type::Str, ExpressionKind::String(value.clone())),
        E::Name(name) => (
            locals
                .get(name)
                .ok_or_else(|| LowerError(format!("unknown local `{name}`")))?
                .0
                .clone(),
            ExpressionKind::Local(name.clone()),
        ),
        E::Call { callee, arguments } => {
            let (params, result) = signatures
                .get(callee)
                .ok_or_else(|| LowerError(format!("unknown function `{callee}`")))?;
            if params.len() != arguments.len() {
                return Err(LowerError(format!("wrong argument count for `{callee}`")));
            }
            let args = arguments
                .iter()
                .zip(params)
                .map(|(arg, ty)| lower_expr(arg, Some(ty), locals, signatures))
                .collect::<Result<_, _>>()?;
            (
                result.clone(),
                ExpressionKind::Call {
                    callee: callee.clone(),
                    arguments: args,
                },
            )
        }
        E::Unary { op, value } => {
            let expected = if *op == UnaryOp::Not {
                Some(&Type::Bool)
            } else {
                hint
            };
            let value = Box::new(lower_expr(value, expected, locals, signatures)?);
            let ty = if *op == UnaryOp::Not {
                Type::Bool
            } else {
                value.ty.clone()
            };
            (ty, ExpressionKind::Unary { op: *op, value })
        }
        E::Binary { left, op, right } => {
            let comparison = matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
            );
            let logical = matches!(op, BinaryOp::And | BinaryOp::Or);
            let inferred_hint = if logical {
                Some(Type::Bool)
            } else if hint.is_none() && is_const_expr(left) {
                Some(expression_type(right, locals, signatures)?)
            } else {
                hint.cloned()
            };
            let operand_hint = inferred_hint.as_ref();
            let left = Box::new(lower_expr(left, operand_hint, locals, signatures)?);
            let right = Box::new(lower_expr(right, Some(&left.ty), locals, signatures)?);
            (
                if comparison || logical {
                    Type::Bool
                } else {
                    left.ty.clone()
                },
                ExpressionKind::Binary {
                    left,
                    op: *op,
                    right,
                },
            )
        }
    };
    Ok(Expression { ty, kind })
}

fn is_const_expr(expr: &ast::Expression) -> bool {
    match expr {
        ast::Expression::Integer(_) => true,
        ast::Expression::Unary {
            op: UnaryOp::Negate,
            value,
        } => is_const_expr(value),
        ast::Expression::Binary { left, op, right } => {
            matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Remainder
            ) && is_const_expr(left)
                && is_const_expr(right)
        }
        _ => false,
    }
}

fn expression_type(
    expr: &ast::Expression,
    locals: &Locals,
    signatures: &Signatures,
) -> Result<Type, LowerError> {
    match expr {
        ast::Expression::Unit => Ok(Type::Unit),
        ast::Expression::Integer(_) => Ok(Type::I32),
        ast::Expression::Boolean(_) => Ok(Type::Bool),
        ast::Expression::String(_) => Ok(Type::Str),
        ast::Expression::Name(name) => locals
            .get(name)
            .map(|(ty, _)| ty.clone())
            .ok_or_else(|| LowerError(format!("unknown local `{name}`"))),
        ast::Expression::Call { callee, .. } => signatures
            .get(callee)
            .map(|(_, result)| result.clone())
            .ok_or_else(|| LowerError(format!("unknown function `{callee}`"))),
        ast::Expression::Unary { op, value } => {
            if *op == UnaryOp::Not {
                Ok(Type::Bool)
            } else {
                expression_type(value, locals, signatures)
            }
        }
        ast::Expression::Binary { left, op, .. } => {
            if matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or
            ) {
                Ok(Type::Bool)
            } else {
                expression_type(left, locals, signatures)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_checked_program_to_typed_hir_without_rust_backend_nodes() {
        let source = "mod sample\nfn add(a: i32, b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let sum: i32 = a + b\n    return sum\n";
        let ast = irq_parser::parse(source).unwrap();
        irq_check::check(&ast).unwrap();
        let hir = lower(&ast).unwrap();
        assert_eq!(hir.functions[0].return_type, Type::I32);
        let Statement::Let { value, .. } = &hir.functions[0].body[0] else {
            panic!("expected HIR let")
        };
        assert_eq!(value.ty, Type::I32);
        assert!(matches!(
            value.kind,
            ExpressionKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn contextual_literal_in_left_operand_inherits_wide_type() {
        let source = "mod sample\nfn add_one(value: i64) -> i64\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 1 + value\n";
        let ast = irq_parser::parse(source).unwrap();
        irq_check::check(&ast).unwrap();
        let hir = lower(&ast).unwrap();
        let Statement::Return(value) = &hir.functions[0].body[0] else {
            panic!("expected HIR return")
        };
        assert_eq!(value.ty, Type::I64);
        let ExpressionKind::Binary { left, .. } = &value.kind else {
            panic!("expected HIR binary")
        };
        assert_eq!(left.ty, Type::I64);
    }
}
