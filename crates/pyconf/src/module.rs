//! Reading what a module binds at its top level, statement by statement,
//! without running any of it.
//!
//! The reader is honest rather than clever. A top-level `name = literal`
//! binds the name to the literal; a later top-level assignment replaces it,
//! and `del name` removes it. Every other way a module may change a name —
//! an augmented assignment, a method call, an item or attribute assigned,
//! an assignment inside an `if`, a `for` or a `try` — is recorded as a
//! *modification* of the binding, which the reader does not apply: whether
//! it runs, and what it does, is a question only running the module could
//! answer. A name first bound in such a way is bound to something unread.
//!
//! A function's or a class's body is skipped: what it assigns is local to
//! it, and whether it ever runs is unknowable here.

use crate::lexer::{Tokenized, tokenize};
use crate::literal::read_literal;
use crate::position::Span;
use crate::reading::{Binding, BoundValue, ModuleReading};
use crate::token::{Token, TokenKind};

/// Every augmented-assignment operator.
const AUGMENTED: &[&str] = &[
    "+=", "-=", "*=", "/=", "//=", "%=", "**=", ">>=", "<<=", "&=", "|=", "^=", "@=",
];

/// Statements that bind or change nothing at the top level.
const INERT: &[&str] = &[
    "global", "nonlocal", "pass", "break", "continue", "return", "raise", "assert", "yield",
    "await",
];

/// What `source`, a Python module, binds at its top level. Never panics: a
/// module Python would refuse is read up to the statement it fails in.
#[must_use]
pub fn read_module(source: &str) -> ModuleReading {
    let Tokenized { tokens, error } = tokenize(source);
    let mut reader = Reader {
        reading: ModuleReading::default(),
        blocks: Vec::new(),
        pending: None,
    };
    let mut line_start = 0;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Indent => {
                let kind = reader.pending.take().unwrap_or(Block::Control);
                reader.blocks.push(kind);
                line_start = index + 1;
            }
            TokenKind::Dedent => {
                reader.blocks.pop();
                line_start = index + 1;
            }
            TokenKind::Newline => {
                reader.line(&tokens[line_start..index]);
                line_start = index + 1;
            }
            _ => {}
        }
    }
    // Whatever follows the last newline is the statement an error cut
    // short, which is not read.
    reader.reading.error = error;
    reader.reading
}

/// What kind of block a body belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    /// The body of an `if`, a loop, a `try`, a `with` or a `match`, which
    /// runs at most conditionally.
    Control,
    /// The body of a function or a class, whose names are its own.
    Scope,
}

/// Where a statement stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// At the top level: it runs, in order.
    TopLevel,
    /// Inside a control block: it may run, or not, or more than once.
    Conditional,
    /// Inside a function or a class: it binds nothing of the module's.
    Scoped,
}

struct Reader {
    reading: ModuleReading,
    /// The blocks the current line is inside, outermost first.
    blocks: Vec<Block>,
    /// The kind of block the line just read opened, which the next indent
    /// enters.
    pending: Option<Block>,
}

impl Reader {
    fn context(&self) -> Context {
        if self.blocks.contains(&Block::Scope) {
            Context::Scoped
        } else if self.blocks.is_empty() {
            Context::TopLevel
        } else {
            Context::Conditional
        }
    }

    /// Reads one logical line.
    fn line(&mut self, tokens: &[Token]) {
        self.pending = None;
        let Some(first) = tokens.first() else {
            return;
        };
        if first.kind.is_op("@") {
            return;
        }
        if let Some(block) = compound_block(tokens) {
            self.compound(tokens, block);
            return;
        }
        let context = self.context();
        if context == Context::Scoped {
            return;
        }
        for statement in split_top_level(tokens, ";") {
            self.simple(statement, context);
        }
    }

    /// Reads a compound statement's header and any body written on the same
    /// line, and remembers which kind of block its indented body opens.
    fn compound(&mut self, tokens: &[Token], block: Block) {
        let outer = self.context();
        let keyword_at = usize::from(tokens[0].kind.is_name("async"));
        let colon = top_level_positions(tokens, |kind| kind.is_op(":"))
            .into_iter()
            .find(|&index| index > keyword_at);
        let header = &tokens[..colon.unwrap_or(tokens.len())];
        if outer != Context::Scoped {
            self.header_bindings(header, keyword_at, outer);
        }
        let suite = colon.map_or(&[][..], |colon| &tokens[colon + 1..]);
        if suite.is_empty() {
            self.pending = Some(block);
            return;
        }
        self.blocks.push(block);
        let inner = self.context();
        if inner != Context::Scoped {
            for statement in split_top_level(suite, ";") {
                self.simple(statement, inner);
            }
        }
        self.blocks.pop();
    }

    /// Binds the names a compound statement's header binds: a function's or
    /// a class's name, a loop's targets, a `with`'s or an `except`'s `as`.
    fn header_bindings(&mut self, header: &[Token], keyword_at: usize, context: Context) {
        let Some(span) = statement_span(header) else {
            return;
        };
        let keyword = match &header[keyword_at].kind {
            TokenKind::Name(keyword) => keyword.as_str(),
            _ => return,
        };
        match keyword {
            "def" | "class" => {
                if let Some(Token {
                    kind: TokenKind::Name(name),
                    span: target,
                }) = header.get(keyword_at + 1)
                {
                    self.assign(name, *target, BoundValue::Unread(span), context, span);
                }
            }
            "for" => {
                let start = keyword_at + 1;
                let end = top_level_positions(header, |kind| kind.is_name("in"))
                    .into_iter()
                    .find(|&index| index > start)
                    .unwrap_or(header.len());
                if start < end {
                    self.unpack(&header[start..end], span, Context::Conditional);
                }
            }
            "with" | "except" => {
                for index in top_level_positions(header, |kind| kind.is_name("as")) {
                    if let Some(Token {
                        kind: TokenKind::Name(name),
                        span: target,
                    }) = header.get(index + 1)
                    {
                        self.assign(
                            name,
                            *target,
                            BoundValue::Unread(span),
                            Context::Conditional,
                            span,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Reads one simple statement.
    fn simple(&mut self, statement: &[Token], context: Context) {
        let (Some(first), Some(span)) = (statement.first(), statement_span(statement)) else {
            return;
        };
        match &first.kind {
            TokenKind::Name(keyword) if keyword == "import" => {
                self.import(&statement[1..], context);
                return;
            }
            TokenKind::Name(keyword) if keyword == "from" => {
                self.import_from(statement, context);
                return;
            }
            TokenKind::Name(keyword) if keyword == "del" => {
                for target in split_top_level(&statement[1..], ",") {
                    self.delete(target, context, span);
                }
                return;
            }
            TokenKind::Name(keyword) if INERT.contains(&keyword.as_str()) => return,
            _ => {}
        }
        if let Some(operator) =
            top_level_positions(statement, |kind| AUGMENTED.iter().any(|op| kind.is_op(op))).first()
        {
            if let Some(name) = target_name(&statement[..*operator]) {
                self.modify(name, span);
            }
            return;
        }
        // A lambda's default (`f = lambda x=1: x`) is no assignment.
        let lambda = top_level_positions(statement, |kind| kind.is_name("lambda"))
            .first()
            .copied()
            .unwrap_or(statement.len());
        let equals: Vec<usize> = top_level_positions(statement, |kind| kind.is_op("="))
            .into_iter()
            .filter(|&index| index < lambda)
            .collect();
        let Some(&last) = equals.last() else {
            // An expression: a method call or an item read on a name may
            // change it (`exclude_patterns.append(…)`).
            if let (TokenKind::Name(name), Some(next)) = (&first.kind, statement.get(1))
                && (next.kind.is_op(".") || next.kind.is_op("["))
            {
                self.modify(name, span);
            }
            return;
        };
        let value_tokens = &statement[last + 1..];
        let value = read_literal(value_tokens).map_or_else(
            || BoundValue::Unread(statement_span(value_tokens).unwrap_or(span)),
            BoundValue::Literal,
        );
        let mut start = 0;
        for &end in &equals {
            let mut target = &statement[start..end];
            // An annotation: `name: type = value`.
            if let Some(&colon) = top_level_positions(target, |kind| kind.is_op(":")).first() {
                target = &target[..colon];
            }
            self.assign_target(target, value.clone(), context, span);
            start = end + 1;
        }
    }

    /// Binds what an assignment's `target` names to `value`.
    fn assign_target(&mut self, target: &[Token], value: BoundValue, context: Context, span: Span) {
        if let [
            Token {
                kind: TokenKind::Name(name),
                span: at,
            },
        ] = target
        {
            self.assign(name, *at, value, context, span);
            return;
        }
        let unpacking = !top_level_positions(target, |kind| kind.is_op(",")).is_empty()
            || unwrap_brackets(target).len() < target.len();
        match base_name(target) {
            // An attribute or an item of the name.
            Some(name) if !unpacking => self.modify(name, span),
            _ => self.unpack(target, span, context),
        }
    }

    /// Binds every name an unpacking target names (`version, release = …`,
    /// `for role, name in …`) to something unread: which element each gets
    /// is not followed.
    fn unpack(&mut self, target: &[Token], span: Span, context: Context) {
        let inner = unwrap_brackets(target);
        for element in split_top_level(inner, ",") {
            let element = match element.first() {
                Some(star) if star.kind.is_op("*") => &element[1..],
                _ => element,
            };
            match element {
                [] => {}
                [
                    Token {
                        kind: TokenKind::Name(name),
                        span: at,
                    },
                ] => self.assign(name, *at, BoundValue::Unread(span), context, span),
                _ => match base_name(element) {
                    Some(name) => self.modify(name, span),
                    None if element.len() < inner.len() => self.unpack(element, span, context),
                    None => {}
                },
            }
        }
    }

    /// Records that `name` is bound to `value` by the statement at `span`.
    fn assign(
        &mut self,
        name: &str,
        target: Span,
        value: BoundValue,
        context: Context,
        span: Span,
    ) {
        match context {
            Context::TopLevel => {
                self.reading.bindings.insert(
                    name.to_string(),
                    Binding {
                        target,
                        value,
                        modifications: Vec::new(),
                    },
                );
            }
            Context::Conditional => match self.reading.bindings.get_mut(name) {
                Some(binding) => binding.modifications.push(span),
                None => {
                    self.reading.bindings.insert(
                        name.to_string(),
                        Binding {
                            target,
                            value: BoundValue::Unread(span),
                            modifications: Vec::new(),
                        },
                    );
                }
            },
            Context::Scoped => {}
        }
    }

    /// Records that the statement at `span` may change `name`'s value.
    fn modify(&mut self, name: &str, span: Span) {
        match self.reading.bindings.get_mut(name) {
            Some(binding) => binding.modifications.push(span),
            None => {
                self.reading.bindings.insert(
                    name.to_string(),
                    Binding {
                        target: span,
                        value: BoundValue::Unread(span),
                        modifications: Vec::new(),
                    },
                );
            }
        }
    }

    /// `del target`: at the top level, a name is unbound; anywhere else it
    /// may be.
    fn delete(&mut self, target: &[Token], context: Context, span: Span) {
        match (target, context) {
            (
                [
                    Token {
                        kind: TokenKind::Name(name),
                        ..
                    },
                ],
                Context::TopLevel,
            ) => {
                self.reading.bindings.remove(name);
            }
            _ => {
                if let Some(name) = target_name(target) {
                    self.modify(name, span);
                }
            }
        }
    }

    /// `import a.b, c as d`: binds `a` and `d`.
    fn import(&mut self, items: &[Token], context: Context) {
        for item in split_top_level(items, ",") {
            let alias = aliased_name(item).or_else(|| match item.first() {
                Some(Token {
                    kind: TokenKind::Name(name),
                    span,
                }) => Some((name.as_str(), *span)),
                _ => None,
            });
            if let Some((name, target)) = alias {
                let span = statement_span(item).unwrap_or(target);
                self.assign(name, target, BoundValue::Unread(span), context, span);
            }
        }
    }

    /// `from m import a, b as c` binds `a` and `c`; `from m import *` may
    /// bind anything.
    fn import_from(&mut self, statement: &[Token], context: Context) {
        let Some(import) = statement
            .iter()
            .position(|token| token.kind.is_name("import"))
        else {
            return;
        };
        let names = unwrap_brackets(&statement[import + 1..]);
        if matches!(names, [star] if star.kind.is_op("*")) {
            self.reading
                .wildcard_imports
                .push(statement_span(statement).unwrap_or(names[0].span));
            return;
        }
        for item in split_top_level(names, ",") {
            let alias = aliased_name(item).or(match item {
                [
                    Token {
                        kind: TokenKind::Name(name),
                        span,
                    },
                ] => Some((name.as_str(), *span)),
                _ => None,
            });
            if let Some((name, target)) = alias {
                let span = statement_span(item).unwrap_or(target);
                self.assign(name, target, BoundValue::Unread(span), context, span);
            }
        }
    }
}

/// The kind of block a line opens, when it is a compound statement's
/// header.
fn compound_block(tokens: &[Token]) -> Option<Block> {
    let keyword = match &tokens.first()?.kind {
        TokenKind::Name(keyword) => keyword.as_str(),
        _ => return None,
    };
    let keyword = if keyword == "async" {
        match &tokens.get(1)?.kind {
            TokenKind::Name(next) => next.as_str(),
            _ => return None,
        }
    } else {
        keyword
    };
    match keyword {
        "def" | "class" => Some(Block::Scope),
        "if" | "elif" | "else" | "while" | "for" | "try" | "except" | "finally" | "with" => {
            Some(Block::Control)
        }
        // Soft keywords: a header only when the line ends in its colon, so
        // `match = 1` stays an assignment.
        "match" | "case"
            if tokens.len() > 2 && tokens.last().is_some_and(|token| token.kind.is_op(":")) =>
        {
            Some(Block::Control)
        }
        _ => None,
    }
}

/// The name an attribute or item target is reached through: `a` for `a.b`
/// or `a[0]`.
fn base_name(target: &[Token]) -> Option<&str> {
    match target {
        [
            Token {
                kind: TokenKind::Name(name),
                ..
            },
            next,
            ..,
        ] if next.kind.is_op(".") || next.kind.is_op("[") => Some(name),
        _ => None,
    }
}

/// The name a target changes: the name itself, or the one an attribute or
/// item is reached through.
fn target_name(target: &[Token]) -> Option<&str> {
    match target {
        [
            Token {
                kind: TokenKind::Name(name),
                ..
            },
        ] => Some(name),
        _ => base_name(target),
    }
}

/// The alias of an `x as y` item, and where it is written.
fn aliased_name(item: &[Token]) -> Option<(&str, Span)> {
    let as_at = item.iter().position(|token| token.kind.is_name("as"))?;
    match item.get(as_at + 1)? {
        Token {
            kind: TokenKind::Name(alias),
            span,
        } => Some((alias, *span)),
        _ => None,
    }
}

/// `tokens` without one pair of brackets enclosing all of them.
fn unwrap_brackets(tokens: &[Token]) -> &[Token] {
    let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
        return tokens;
    };
    let pair = (first.kind.is_op("(") && last.kind.is_op(")"))
        || (first.kind.is_op("[") && last.kind.is_op("]"));
    if pair && tokens.len() >= 2 && closing_index(tokens) == Some(tokens.len() - 1) {
        &tokens[1..tokens.len() - 1]
    } else {
        tokens
    }
}

/// Where the bracket opening `tokens` closes.
fn closing_index(tokens: &[Token]) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if is_opening(&token.kind) {
            depth += 1;
        } else if is_closing(&token.kind) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn is_opening(kind: &TokenKind) -> bool {
    kind.is_op("(") || kind.is_op("[") || kind.is_op("{")
}

fn is_closing(kind: &TokenKind) -> bool {
    kind.is_op(")") || kind.is_op("]") || kind.is_op("}")
}

/// The indices of the tokens outside every bracket that satisfy `wanted`.
fn top_level_positions(tokens: &[Token], wanted: impl Fn(&TokenKind) -> bool) -> Vec<usize> {
    let mut depth = 0usize;
    let mut found = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if is_opening(&token.kind) {
            depth += 1;
        } else if is_closing(&token.kind) {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && wanted(&token.kind) {
            found.push(index);
        }
    }
    found
}

/// `tokens` split at each top-level `op`.
fn split_top_level<'a>(tokens: &'a [Token], op: &str) -> Vec<&'a [Token]> {
    let mut parts = Vec::new();
    let mut start = 0;
    for index in top_level_positions(tokens, |kind| kind.is_op(op)) {
        parts.push(&tokens[start..index]);
        start = index + 1;
    }
    parts.push(&tokens[start..]);
    parts
}

/// The range `tokens` cover, if any.
fn statement_span(tokens: &[Token]) -> Option<Span> {
    Some(tokens.first()?.span.to(tokens.last()?.span))
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod block_tests;
#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod modification_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod test_support;
