//! Expression types for arithmetic and conditional expressions.

use crate::word::Word;
use std::fmt;

/// An expression used in arithmetic (( )) or conditional [[ ]] contexts.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // Literals
    /// Integer literal
    Integer(i64),
    /// String literal
    String(String),
    /// Variable reference
    Variable(String),

    // Arithmetic operators
    /// Addition
    Add(Box<Expr>, Box<Expr>),
    /// Subtraction
    Sub(Box<Expr>, Box<Expr>),
    /// Multiplication
    Mul(Box<Expr>, Box<Expr>),
    /// Division
    Div(Box<Expr>, Box<Expr>),
    /// Modulo
    Mod(Box<Expr>, Box<Expr>),
    /// Exponentiation
    Pow(Box<Expr>, Box<Expr>),

    // Bitwise operators
    /// Bitwise AND
    BitAnd(Box<Expr>, Box<Expr>),
    /// Bitwise OR
    BitOr(Box<Expr>, Box<Expr>),
    /// Bitwise XOR
    BitXor(Box<Expr>, Box<Expr>),
    /// Bitwise NOT
    BitNot(Box<Expr>),
    /// Left shift
    Shl(Box<Expr>, Box<Expr>),
    /// Right shift
    Shr(Box<Expr>, Box<Expr>),

    // Logical operators
    /// Logical AND
    And(Box<Expr>, Box<Expr>),
    /// Logical OR
    Or(Box<Expr>, Box<Expr>),
    /// Logical NOT
    Not(Box<Expr>),

    // Comparison operators
    /// Equal
    Eq(Box<Expr>, Box<Expr>),
    /// Not equal
    Ne(Box<Expr>, Box<Expr>),
    /// Less than
    Lt(Box<Expr>, Box<Expr>),
    /// Less than or equal
    Le(Box<Expr>, Box<Expr>),
    /// Greater than
    Gt(Box<Expr>, Box<Expr>),
    /// Greater than or equal
    Ge(Box<Expr>, Box<Expr>),

    // Assignment operators
    /// Simple assignment
    Assign(Box<Expr>, Box<Expr>),
    /// Add and assign
    AddAssign(Box<Expr>, Box<Expr>),
    /// Subtract and assign
    SubAssign(Box<Expr>, Box<Expr>),
    /// Multiply and assign
    MulAssign(Box<Expr>, Box<Expr>),
    /// Divide and assign
    DivAssign(Box<Expr>, Box<Expr>),
    /// Modulo and assign
    ModAssign(Box<Expr>, Box<Expr>),

    // Increment/Decrement
    /// Pre-increment: ++var
    PreInc(Box<Expr>),
    /// Post-increment: var++
    PostInc(Box<Expr>),
    /// Pre-decrement: --var
    PreDec(Box<Expr>),
    /// Post-decrement: var--
    PostDec(Box<Expr>),

    // Conditional (ternary)
    /// Ternary: condition ? true_expr : false_expr
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),

    // String operators (for [[ ]])
    /// String comparison: == (with pattern)
    StringEq(Box<Expr>, Box<Expr>),
    /// String pattern match: =~
    StringMatch(Box<Expr>, Box<Expr>),
    /// String length: -n STRING
    StringNonEmpty(Box<Expr>),
    /// String empty: -z STRING
    StringEmpty(Box<Expr>),

    // File test operators
    /// File exists: -e FILE
    FileExists(Box<Expr>),
    /// Regular file: -f FILE
    IsRegularFile(Box<Expr>),
    /// Directory: -d FILE
    IsDirectory(Box<Expr>),
    /// Symbolic link: -L FILE
    IsSymlink(Box<Expr>),
    /// Readable: -r FILE
    IsReadable(Box<Expr>),
    /// Writable: -w FILE
    IsWritable(Box<Expr>),
    /// Executable: -x FILE
    IsExecutable(Box<Expr>),
    /// File is non-empty: -s FILE
    IsNonEmpty(Box<Expr>),
    /// File1 is newer than file2: FILE1 -nt FILE2
    IsNewer(Box<Expr>, Box<Expr>),
    /// File1 is older than file2: FILE1 -ot FILE2
    IsOlder(Box<Expr>, Box<Expr>),

    // Special
    /// Command substitution result
    CommandSubst(String),
    /// Arithmetic expansion
    Arithmetic(Box<Expr>),
    /// Grouping with parentheses
    Group(Box<Expr>),
}

impl Expr {
    /// Create an integer literal.
    pub fn int(n: i64) -> Self {
        Expr::Integer(n)
    }

    /// Create a string literal.
    pub fn string(s: impl Into<String>) -> Self {
        Expr::String(s.into())
    }

    /// Create a variable reference.
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Variable(name.into())
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Integer(n) => write!(f, "{}", n),
            Expr::String(s) => write!(f, "{}", s),
            Expr::Variable(name) => write!(f, "{}", name),
            Expr::Add(l, r) => write!(f, "{} + {}", l, r),
            Expr::Sub(l, r) => write!(f, "{} - {}", l, r),
            Expr::Mul(l, r) => write!(f, "{} * {}", l, r),
            Expr::Div(l, r) => write!(f, "{} / {}", l, r),
            Expr::Mod(l, r) => write!(f, "{} % {}", l, r),
            Expr::Pow(l, r) => write!(f, "{} ** {}", l, r),
            Expr::BitAnd(l, r) => write!(f, "{} & {}", l, r),
            Expr::BitOr(l, r) => write!(f, "{} | {}", l, r),
            Expr::BitXor(l, r) => write!(f, "{} ^ {}", l, r),
            Expr::BitNot(e) => write!(f, "~{}", e),
            Expr::Shl(l, r) => write!(f, "{} << {}", l, r),
            Expr::Shr(l, r) => write!(f, "{} >> {}", l, r),
            Expr::And(l, r) => write!(f, "{} && {}", l, r),
            Expr::Or(l, r) => write!(f, "{} || {}", l, r),
            Expr::Not(e) => write!(f, "!{}", e),
            Expr::Eq(l, r) => write!(f, "{} == {}", l, r),
            Expr::Ne(l, r) => write!(f, "{} != {}", l, r),
            Expr::Lt(l, r) => write!(f, "{} < {}", l, r),
            Expr::Le(l, r) => write!(f, "{} <= {}", l, r),
            Expr::Gt(l, r) => write!(f, "{} > {}", l, r),
            Expr::Ge(l, r) => write!(f, "{} >= {}", l, r),
            Expr::Assign(l, r) => write!(f, "{} = {}", l, r),
            Expr::AddAssign(l, r) => write!(f, "{} += {}", l, r),
            Expr::SubAssign(l, r) => write!(f, "{} -= {}", l, r),
            Expr::MulAssign(l, r) => write!(f, "{} *= {}", l, r),
            Expr::DivAssign(l, r) => write!(f, "{} /= {}", l, r),
            Expr::ModAssign(l, r) => write!(f, "{} %= {}", l, r),
            Expr::PreInc(e) => write!(f, "++{}", e),
            Expr::PostInc(e) => write!(f, "{}++", e),
            Expr::PreDec(e) => write!(f, "--{}", e),
            Expr::PostDec(e) => write!(f, "{}--", e),
            Expr::Ternary(c, t, e) => write!(f, "{} ? {} : {}", c, t, e),
            Expr::StringEq(l, r) => write!(f, "{} == {}", l, r),
            Expr::StringMatch(l, r) => write!(f, "{} =~ {}", l, r),
            Expr::StringNonEmpty(e) => write!(f, "-n {}", e),
            Expr::StringEmpty(e) => write!(f, "-z {}", e),
            Expr::FileExists(e) => write!(f, "-e {}", e),
            Expr::IsRegularFile(e) => write!(f, "-f {}", e),
            Expr::IsDirectory(e) => write!(f, "-d {}", e),
            Expr::IsSymlink(e) => write!(f, "-L {}", e),
            Expr::IsReadable(e) => write!(f, "-r {}", e),
            Expr::IsWritable(e) => write!(f, "-w {}", e),
            Expr::IsExecutable(e) => write!(f, "-x {}", e),
            Expr::IsNonEmpty(e) => write!(f, "-s {}", e),
            Expr::IsNewer(l, r) => write!(f, "{} -nt {}", l, r),
            Expr::IsOlder(l, r) => write!(f, "{} -ot {}", l, r),
            Expr::CommandSubst(cmd) => write!(f, "$({})", cmd),
            Expr::Arithmetic(e) => write!(f, "(( {} ))", e),
            Expr::Group(e) => write!(f, "({})", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_integer() {
        let expr = Expr::int(42);
        assert_eq!(expr.to_string(), "42");
    }

    #[test]
    fn test_expr_variable() {
        let expr = Expr::var("x");
        assert_eq!(expr.to_string(), "x");
    }

    #[test]
    fn test_expr_arithmetic() {
        let expr = Expr::Add(Box::new(Expr::int(1)), Box::new(Expr::int(2)));
        assert_eq!(expr.to_string(), "1 + 2");
    }

    #[test]
    fn test_expr_comparison() {
        let expr = Expr::Lt(Box::new(Expr::var("x")), Box::new(Expr::int(10)));
        assert_eq!(expr.to_string(), "x < 10");
    }

    #[test]
    fn test_expr_ternary() {
        let expr = Expr::Ternary(
            Box::new(Expr::var("x")),
            Box::new(Expr::int(1)),
            Box::new(Expr::int(0)),
        );
        assert_eq!(expr.to_string(), "x ? 1 : 0");
    }

    #[test]
    fn test_expr_file_test() {
        let expr = Expr::FileExists(Box::new(Expr::string("/tmp/file")));
        assert_eq!(expr.to_string(), "-e /tmp/file");
    }
}
