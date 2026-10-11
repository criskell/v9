#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum Token {
    TemplateSpan,
    TemplateTail,
    Period,
    LeftBracket,
    QuestionPeriod,
    LeftParen,
    RightParen,
    RightBracket,
    LeftBrace,
    Colon,
    Ellipsis,
    Conditional,
    Semicolon,
    RightBrace,
    Eos,

    Arrow,
    Init,
    Assign,
    AssignNullish,
    AssignOr,
    AssignAnd,
    AssignBitOr,
    AssignBitXor,
    AssignBitAnd,
    AssignShl,
    AssignSar,
    AssignShr,
    AssignMul,
    AssignDiv,
    AssignMod,
    AssignExp,
    AssignAdd,
    AssignSub,

    Comma,
    Nullish,
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Sar,
    Shr,
    Mul,
    Div,
    Mod,
    Exp,
    Add,
    Sub,
    Not,
    BitNot,
    Delete,
    TypeOf,
    Void,
    Inc,
    Dec,

    Eq,
    EqStrict,
    NotEq,
    NotEqStrict,
    LessThan,
    GreaterThan,
    LessThanEq,
    GreaterThanEq,
    InstanceOf,
    In,

    Break,
    Case,
    Catch,
    Continue,
    Debugger,
    Default,
    Do,
    Else,
    Finally,
    For,
    Function,
    If,
    New,
    Return,
    Switch,
    Throw,
    Try,
    Var,
    While,
    With,
    This,

    NullLiteral,
    TrueLiteral,
    FalseLiteral,
    Number,
    Smi,
    BigInt,
    String,
    Super,
    Identifier,
    Get,
    Set,
    Using,
    Of,
    Accessor,
    Async,
    Await,
    Yield,
    Let,
    Static,
    FutureStrictReservedWord,
    EscapedStrictReservedWord,
    Enum,
    Class,
    Const,
    Export,
    Extends,
    Import,
    PrivateName,

    Illegal,
    EscapedKeyword,

    Whitespace,
    Uninitialized,
    RegExpLiteral,
}

use Token::*;

macro_rules! token_table {
    ($($token:ident, $string:expr, $precedence:expr, $keyword:expr;)*) => {
        impl Token {
            pub fn name(self) -> &'static str {
                match self { $($token => stringify!($token),)* }
            }

            pub fn string(self) -> Option<&'static str> {
                match self { $($token => $string,)* }
            }

            pub fn precedence(self, accept_in: bool) -> i32 {
                if self == In && !accept_in {
                    return 0;
                }
                match self { $($token => $precedence,)* }
            }

            pub fn is_keyword(self) -> bool {
                match self { $($token => $keyword,)* }
            }

            pub const ALL: &'static [Token] = &[$($token,)*];
        }
    };
}

token_table! {
    TemplateSpan,              None,              0, false;
    TemplateTail,              None,              0, false;
    Period,                    Some("."),         0, false;
    LeftBracket,               Some("["),         0, false;
    QuestionPeriod,            Some("?."),        0, false;
    LeftParen,                 Some("("),         0, false;
    RightParen,                Some(")"),         0, false;
    RightBracket,              Some("]"),         0, false;
    LeftBrace,                 Some("{"),         0, false;
    Colon,                     Some(":"),         0, false;
    Ellipsis,                  Some("..."),       0, false;
    Conditional,               Some("?"),         3, false;
    Semicolon,                 Some(";"),         0, false;
    RightBrace,                Some("}"),         0, false;
    Eos,                       Some("EOS"),       0, false;
    Arrow,                     Some("=>"),        0, false;
    Init,                      Some("=init"),     2, false;
    Assign,                    Some("="),         2, false;
    AssignNullish,             Some("??="),       2, false;
    AssignOr,                  Some("||="),       2, false;
    AssignAnd,                 Some("&&="),       2, false;
    AssignBitOr,               Some("|="),        2, false;
    AssignBitXor,              Some("^="),        2, false;
    AssignBitAnd,              Some("&="),        2, false;
    AssignShl,                 Some("<<="),       2, false;
    AssignSar,                 Some(">>="),       2, false;
    AssignShr,                 Some(">>>="),      2, false;
    AssignMul,                 Some("*="),        2, false;
    AssignDiv,                 Some("/="),        2, false;
    AssignMod,                 Some("%="),        2, false;
    AssignExp,                 Some("**="),       2, false;
    AssignAdd,                 Some("+="),        2, false;
    AssignSub,                 Some("-="),        2, false;
    Comma,                     Some(","),         1, false;
    Nullish,                   Some("??"),        3, false;
    Or,                        Some("||"),        4, false;
    And,                       Some("&&"),        5, false;
    BitOr,                     Some("|"),         6, false;
    BitXor,                    Some("^"),         7, false;
    BitAnd,                    Some("&"),         8, false;
    Shl,                       Some("<<"),       11, false;
    Sar,                       Some(">>"),       11, false;
    Shr,                       Some(">>>"),      11, false;
    Mul,                       Some("*"),        13, false;
    Div,                       Some("/"),        13, false;
    Mod,                       Some("%"),        13, false;
    Exp,                       Some("**"),       14, false;
    Add,                       Some("+"),        12, false;
    Sub,                       Some("-"),        12, false;
    Not,                       Some("!"),         0, false;
    BitNot,                    Some("~"),         0, false;
    Delete,                    Some("delete"),    0, true;
    TypeOf,                    Some("typeof"),    0, true;
    Void,                      Some("void"),      0, true;
    Inc,                       Some("++"),        0, false;
    Dec,                       Some("--"),        0, false;
    Eq,                        Some("=="),        9, false;
    EqStrict,                  Some("==="),       9, false;
    NotEq,                     Some("!="),        9, false;
    NotEqStrict,               Some("!=="),       9, false;
    LessThan,                  Some("<"),        10, false;
    GreaterThan,               Some(">"),        10, false;
    LessThanEq,                Some("<="),       10, false;
    GreaterThanEq,             Some(">="),       10, false;
    InstanceOf,                Some("instanceof"), 10, true;
    In,                        Some("in"),       10, true;
    Break,                     Some("break"),     0, true;
    Case,                      Some("case"),      0, true;
    Catch,                     Some("catch"),     0, true;
    Continue,                  Some("continue"),  0, true;
    Debugger,                  Some("debugger"),  0, true;
    Default,                   Some("default"),   0, true;
    Do,                        Some("do"),        0, true;
    Else,                      Some("else"),      0, true;
    Finally,                   Some("finally"),   0, true;
    For,                       Some("for"),       0, true;
    Function,                  Some("function"),  0, true;
    If,                        Some("if"),        0, true;
    New,                       Some("new"),       0, true;
    Return,                    Some("return"),    0, true;
    Switch,                    Some("switch"),    0, true;
    Throw,                     Some("throw"),     0, true;
    Try,                       Some("try"),       0, true;
    Var,                       Some("var"),       0, true;
    While,                     Some("while"),     0, true;
    With,                      Some("with"),      0, true;
    This,                      Some("this"),      0, true;
    NullLiteral,               Some("null"),      0, true;
    TrueLiteral,               Some("true"),      0, true;
    FalseLiteral,              Some("false"),     0, true;
    Number,                    None,              0, false;
    Smi,                       None,              0, false;
    BigInt,                    None,              0, false;
    String,                    None,              0, false;
    Super,                     Some("super"),     0, true;
    Identifier,                None,              0, false;
    Get,                       Some("get"),       0, true;
    Set,                       Some("set"),       0, true;
    Using,                     Some("using"),     0, true;
    Of,                        Some("of"),        0, true;
    Accessor,                  Some("accessor"),  0, true;
    Async,                     Some("async"),     0, true;
    Await,                     Some("await"),     0, true;
    Yield,                     Some("yield"),     0, true;
    Let,                       Some("let"),       0, true;
    Static,                    Some("static"),    0, true;
    FutureStrictReservedWord,  None,              0, false;
    EscapedStrictReservedWord, None,              0, false;
    Enum,                      Some("enum"),      0, true;
    Class,                     Some("class"),     0, true;
    Const,                     Some("const"),     0, true;
    Export,                    Some("export"),    0, true;
    Extends,                   Some("extends"),   0, true;
    Import,                    Some("import"),    0, true;
    PrivateName,               None,              0, false;
    Illegal,                   Some("ILLEGAL"),   0, false;
    EscapedKeyword,            None,              0, false;
    Whitespace,                None,              0, false;
    Uninitialized,             None,              0, false;
    RegExpLiteral,             None,              0, false;
}

impl Token {
    #[inline]
    const fn in_range(self, low: Token, high: Token) -> bool {
        (self as u8) >= (low as u8) && (self as u8) <= (high as u8)
    }

    #[inline]
    pub const fn is_auto_semicolon(self) -> bool {
        self.in_range(Semicolon, Eos)
    }

    #[inline]
    pub const fn is_member(self) -> bool {
        self.in_range(TemplateSpan, LeftBracket)
    }

    #[inline]
    pub const fn is_property(self) -> bool {
        self.in_range(Period, LeftBracket)
    }

    #[inline]
    pub const fn is_property_or_call(self) -> bool {
        self.in_range(TemplateSpan, LeftParen)
    }

    #[inline]
    pub const fn is_assignment_op(self) -> bool {
        self.in_range(Init, AssignSub)
    }

    #[inline]
    pub const fn is_logical_assignment_op(self) -> bool {
        self.in_range(AssignNullish, AssignAnd)
    }

    #[inline]
    pub const fn is_arrow_or_assignment_op(self) -> bool {
        self.in_range(Arrow, AssignSub)
    }

    #[inline]
    pub const fn is_binary_op(self) -> bool {
        self.in_range(Comma, Sub)
    }

    #[inline]
    pub const fn is_compare_op(self) -> bool {
        self.in_range(Eq, In)
    }

    #[inline]
    pub const fn is_equality_op(self) -> bool {
        self.in_range(Eq, EqStrict)
    }

    #[inline]
    pub const fn is_ordered_relational_compare_op(self) -> bool {
        self.in_range(LessThan, GreaterThanEq)
    }

    #[inline]
    pub const fn is_unary_op(self) -> bool {
        self.in_range(Add, Void)
    }

    #[inline]
    pub const fn is_count_op(self) -> bool {
        self.in_range(Inc, Dec)
    }

    #[inline]
    pub const fn is_unary_or_count_op(self) -> bool {
        self.in_range(Add, Dec)
    }

    #[inline]
    pub const fn is_shift_op(self) -> bool {
        self.in_range(Shl, Shr)
    }

    #[inline]
    pub const fn is_bit_op(self) -> bool {
        self.in_range(BitOr, Shr) || matches!(self, BitNot)
    }

    #[inline]
    pub const fn is_literal(self) -> bool {
        self.in_range(NullLiteral, String)
    }

    #[inline]
    pub const fn is_any_identifier(self) -> bool {
        self.in_range(Identifier, EscapedStrictReservedWord)
    }

    #[inline]
    pub const fn is_strict_reserved_word(self) -> bool {
        self.in_range(Yield, EscapedStrictReservedWord)
    }

    #[inline]
    pub const fn is_callable(self) -> bool {
        self.in_range(Super, EscapedStrictReservedWord)
    }

    pub fn is_valid_identifier(self, strict: bool, is_generator: bool, is_async: bool) -> bool {
        if self.in_range(Identifier, Async) {
            return true;
        }
        if self == Await {
            return !is_async;
        }
        if self == Yield {
            return !is_generator && !strict;
        }
        self.is_strict_reserved_word() && !strict
    }

    #[inline]
    pub fn is_property_name(self) -> bool {
        self.is_any_identifier() || self.is_keyword()
    }

    pub fn binary_op_for_assignment(self) -> Token {
        debug_assert!(self.in_range(AssignNullish, AssignSub));
        let offset = self as u8 - AssignNullish as u8;
        Token::ALL[Nullish as usize + offset as usize]
    }

    pub const MAX_PRECEDENCE: i32 = 14;
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.string() {
            Some(string) => write!(f, "{string}"),
            None => write!(f, "{}", self.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_every_token_in_order() {
        assert_eq!(Token::ALL.len(), RegExpLiteral as usize + 1);
        for (index, token) in Token::ALL.iter().enumerate() {
            assert_eq!(
                *token as usize,
                index,
                "{} is out of order in the table",
                token.name()
            );
        }
    }

    #[test]
    fn the_two_assignment_blocks_are_parallel() {
        assert_eq!(
            AssignSub as u8 - AssignNullish as u8,
            Sub as u8 - Nullish as u8,
            "the compound assignments and the binary operators must line up"
        );
        for (assignment, expected) in [
            (AssignNullish, Nullish),
            (AssignOr, Or),
            (AssignAnd, And),
            (AssignBitOr, BitOr),
            (AssignShl, Shl),
            (AssignShr, Shr),
            (AssignMul, Mul),
            (AssignExp, Exp),
            (AssignAdd, Add),
            (AssignSub, Sub),
        ] {
            assert_eq!(
                assignment.binary_op_for_assignment(),
                expected,
                "{} should apply {}",
                assignment.name(),
                expected.name()
            );
        }
    }

    #[test]
    fn compound_assignment_spellings_are_their_operator_plus_equals() {
        for token in Token::ALL {
            if !token.in_range(AssignNullish, AssignSub) {
                continue;
            }
            let operator = token.binary_op_for_assignment();
            assert_eq!(
                token.string().unwrap(),
                format!("{}=", operator.string().unwrap()),
                "{} is spelled wrong",
                token.name()
            );
        }
    }

    #[test]
    fn binary_op_range_holds_exactly_the_binary_operators() {
        let binary = [
            Comma, Nullish, Or, And, BitOr, BitXor, BitAnd, Shl, Sar, Shr, Mul, Div, Mod, Exp, Add,
            Sub,
        ];
        for token in Token::ALL {
            assert_eq!(
                token.is_binary_op(),
                binary.contains(token),
                "{} is on the wrong side of the binary-op range",
                token.name()
            );
        }
    }

    #[test]
    fn unary_op_range_holds_exactly_the_unary_operators() {
        let unary = [Add, Sub, Not, BitNot, Delete, TypeOf, Void];
        for token in Token::ALL {
            assert_eq!(
                token.is_unary_op(),
                unary.contains(token),
                "{} is on the wrong side of the unary-op range",
                token.name()
            );
        }
        assert!(Add.is_binary_op() && Add.is_unary_op());
        assert!(Sub.is_binary_op() && Sub.is_unary_op());
        assert!(Mul.is_binary_op() && !Mul.is_unary_op());
        assert!(Not.is_unary_op() && !Not.is_binary_op());
    }

    #[test]
    fn compare_op_range_holds_exactly_the_comparisons() {
        let compare = [
            Eq,
            EqStrict,
            NotEq,
            NotEqStrict,
            LessThan,
            GreaterThan,
            LessThanEq,
            GreaterThanEq,
            InstanceOf,
            In,
        ];
        for token in Token::ALL {
            assert_eq!(
                token.is_compare_op(),
                compare.contains(token),
                "{}",
                token.name()
            );
        }
        assert!(Eq.is_equality_op() && EqStrict.is_equality_op());
        assert!(!NotEq.is_equality_op(), "V8's range stops at ===");
        assert!(LessThan.is_ordered_relational_compare_op());
        assert!(!In.is_ordered_relational_compare_op());
    }

    #[test]
    fn assignment_ranges() {
        assert!(Assign.is_assignment_op());
        assert!(Init.is_assignment_op());
        assert!(AssignAdd.is_assignment_op());
        assert!(AssignNullish.is_logical_assignment_op());
        assert!(AssignAnd.is_logical_assignment_op());
        assert!(
            !AssignBitOr.is_logical_assignment_op(),
            "|= does not short-circuit"
        );
        assert!(!Eq.is_assignment_op(), "== is not an assignment");
        assert!(Arrow.is_arrow_or_assignment_op());
        assert!(!Arrow.is_assignment_op());
    }

    #[test]
    fn precedence_matches_javascript() {
        assert!(Exp.precedence(true) > Mul.precedence(true));
        assert!(Mul.precedence(true) > Add.precedence(true));
        assert!(Add.precedence(true) > Shl.precedence(true) - 1);
        assert!(Shl.precedence(true) > LessThan.precedence(true));
        assert!(LessThan.precedence(true) > Eq.precedence(true));
        assert!(Eq.precedence(true) > BitAnd.precedence(true));
        assert!(BitAnd.precedence(true) > BitXor.precedence(true));
        assert!(BitXor.precedence(true) > BitOr.precedence(true));
        assert!(BitOr.precedence(true) > And.precedence(true));
        assert!(And.precedence(true) > Or.precedence(true));
        assert!(Or.precedence(true) > Nullish.precedence(true));
        assert!(Nullish.precedence(true) > Comma.precedence(true));
        assert_eq!(Mul.precedence(true), Div.precedence(true));
        assert_eq!(Mul.precedence(true), Mod.precedence(true));
        assert_eq!(Shl.precedence(true), Sar.precedence(true));
        assert_eq!(Token::MAX_PRECEDENCE, Exp.precedence(true));
    }

    #[test]
    fn in_loses_its_precedence_inside_a_for_header() {
        assert_eq!(In.precedence(true), 10);
        assert_eq!(In.precedence(false), 0);
        for token in Token::ALL {
            if *token == In {
                continue;
            }
            assert_eq!(
                token.precedence(true),
                token.precedence(false),
                "{} should not depend on accept_in",
                token.name()
            );
        }
    }

    #[test]
    fn keywords_have_spellings_and_literals_mostly_do_not() {
        assert!(Function.is_keyword());
        assert!(
            TypeOf.is_keyword(),
            "typeof is a keyword and a unary operator"
        );
        assert!(!Identifier.is_keyword());
        assert!(!Add.is_keyword());
        for token in Token::ALL {
            if token.is_keyword() {
                assert!(
                    token.string().is_some(),
                    "keyword {} needs a spelling",
                    token.name()
                );
            }
        }
        assert!(Number.string().is_none());
        assert!(Identifier.string().is_none());
    }

    #[test]
    fn literal_range_holds_the_literals() {
        for token in [
            NullLiteral,
            TrueLiteral,
            FalseLiteral,
            Number,
            Smi,
            BigInt,
            String,
        ] {
            assert!(token.is_literal(), "{}", token.name());
        }
        assert!(!Identifier.is_literal());
        assert!(!RegExpLiteral.is_literal(), "V8's range stops at kString");
    }

    #[test]
    fn contextual_keywords_are_usable_as_identifiers() {
        for token in [Identifier, Get, Set, Of, Async, Accessor, Using] {
            assert!(
                token.is_valid_identifier(false, false, false),
                "{} should be usable as an identifier",
                token.name()
            );
        }
        assert!(!Class.is_valid_identifier(false, false, false));
        assert!(!Function.is_valid_identifier(false, false, false));

        assert!(Yield.is_valid_identifier(false, false, false));
        assert!(
            !Yield.is_valid_identifier(true, false, false),
            "strict mode"
        );
        assert!(
            !Yield.is_valid_identifier(false, true, false),
            "in a generator"
        );

        assert!(Await.is_valid_identifier(false, false, false));
        assert!(!Await.is_valid_identifier(false, false, true));
    }

    #[test]
    fn reserved_words_are_still_property_names() {
        assert!(Class.is_property_name());
        assert!(If.is_property_name());
        assert!(Identifier.is_property_name());
        assert!(!LeftBrace.is_property_name());
        assert!(!Number.is_property_name());
    }

    #[test]
    fn statement_terminators() {
        assert!(Semicolon.is_auto_semicolon());
        assert!(RightBrace.is_auto_semicolon());
        assert!(Eos.is_auto_semicolon());
        assert!(!Comma.is_auto_semicolon());
    }

    #[test]
    fn left_hand_side_continuations() {
        assert!(Period.is_property());
        assert!(LeftBracket.is_property());
        assert!(!LeftParen.is_property());
        assert!(LeftParen.is_property_or_call());
        assert!(Period.is_property_or_call());
        assert!(TemplateSpan.is_member());
        assert!(!LeftParen.is_member(), "a call is not a member access");
    }

    #[test]
    fn display_prefers_the_source_spelling() {
        assert_eq!(Add.to_string(), "+");
        assert_eq!(AssignShr.to_string(), ">>>=");
        assert_eq!(Function.to_string(), "function");
        assert_eq!(Identifier.to_string(), "Identifier");
    }
}
