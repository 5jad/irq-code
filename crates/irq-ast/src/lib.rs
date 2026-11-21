//! Backend-independent abstract syntax tree for the IRQ starter language.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub signature: String,
    pub line: usize,
    pub parameters: Vec<Parameter>,
    pub return_type: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Let {
        name: String,
        mutable: bool,
        ty: String,
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
pub enum Expression {
    Unit,
    Integer(i128),
    Boolean(bool),
    String(String),
    Name(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}
