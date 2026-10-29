//! What renaming a definition rewrites — exactly the characters of its name
//! wherever it is written: its definition, and every use the project
//! resolves to it — and why a rename is refused.
//!
//! Where a name stands in a use follows from how the use was written
//! ([`UseForm`]): inside a role's backquotes after any explicit title, at an
//! id on a relation option's line, in a named reference. Every place is then
//! checked against the text before anything is offered: what stands there
//! must still name the definition. A place that does not — a line a Jinja
//! template moved, a file changed on disk unseen, an escape — refuses the
//! whole rename, since an edit missing one use would break a link silently.

use std::fmt;
use std::path::PathBuf;

use rinx_ast::{EntityId, Position, TargetName};
use rinx_index::{DefinitionKey, NameRange};

use crate::project::{UseForm, relation_id_columns};

/// A definition a rename may rewrite: a label, an equation or an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renamed {
    Label(TargetName),
    Equation(TargetName),
    Entity(EntityId),
}

impl Renamed {
    /// The definition `key` names, when a rename may rewrite it — refused
    /// for a document (`None`), a section, a term and a domain object.
    ///
    /// # Errors
    ///
    /// The refusal naming why that kind of definition is not renamed.
    pub fn of(key: Option<&DefinitionKey>) -> Result<Self, RenameRefusal> {
        match key {
            None => Err(RenameRefusal::Document),
            Some(DefinitionKey::Section(_)) => Err(RenameRefusal::Section),
            Some(DefinitionKey::Term(_)) => Err(RenameRefusal::Term),
            Some(DefinitionKey::Object(..)) => Err(RenameRefusal::DomainObject),
            Some(DefinitionKey::Label(name)) => Ok(Self::Label(name.clone())),
            Some(DefinitionKey::Equation(name)) => Ok(Self::Equation(name.clone())),
            Some(DefinitionKey::Entity(id)) => Ok(Self::Entity(id.clone())),
        }
    }

    /// The definition, as the index names it.
    #[must_use]
    pub fn key(&self) -> DefinitionKey {
        match self {
            Self::Label(name) => DefinitionKey::Label(name.clone()),
            Self::Equation(name) => DefinitionKey::Equation(name.clone()),
            Self::Entity(id) => DefinitionKey::Entity(id.clone()),
        }
    }

    /// The name as the index holds it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Label(name) | Self::Equation(name) => name.as_str(),
            Self::Entity(id) => id.as_str(),
        }
    }

    /// Whether `written` names this definition: as the parser normalizes a
    /// label, or exactly as an entity's id — or as its label, which a `:ref:`
    /// to an entity writes.
    #[must_use]
    pub fn is_named_by(&self, written: &str) -> bool {
        let label = TargetName::new(written);
        match self {
            Self::Label(name) | Self::Equation(name) => label == *name,
            Self::Entity(id) => written == id.as_str() || label == TargetName::new(id.as_str()),
        }
    }

    /// `new`, trimmed, when it can stand where this definition's name does,
    /// and the definition it would then be.
    ///
    /// A label may not hold what would end the construct it is written in: a
    /// backquote or an angle bracket ends a role's target, and a colon the
    /// `.. _name:` it is defined by. An entity's id must be an id.
    ///
    /// # Errors
    ///
    /// [`RenameRefusal::InvalidName`], saying what is wrong with it.
    pub fn check_new_name(&self, new: &str) -> Result<(String, Self), RenameRefusal> {
        let new = new.trim();
        let invalid = |reason: &'static str| RenameRefusal::InvalidName {
            name: new.to_string(),
            reason,
        };
        if new.is_empty() {
            return Err(invalid("a name cannot be empty"));
        }
        if new.contains('\n') {
            return Err(invalid("a name is one line"));
        }
        let renamed = match self {
            Self::Entity(_) => {
                Self::Entity(EntityId::new(new).map_err(|_| invalid("it is no entity id"))?)
            }
            Self::Label(_) | Self::Equation(_) if new.contains(['`', '<', '>', ':']) => {
                return Err(invalid(
                    "a label cannot hold a backquote, an angle bracket or a colon",
                ));
            }
            Self::Label(_) => Self::Label(TargetName::new(new)),
            Self::Equation(_) => Self::Equation(TargetName::new(new)),
        };
        Ok((new.to_string(), renamed))
    }
}

/// Where a name stands in a file, and how it is written there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePlace {
    /// Its first character.
    pub start: Position,
    /// Just past its last character.
    pub end: Position,
    /// The name as written there.
    pub written: String,
    /// Whether it is a bare named reference, `name_`, where a name that is
    /// not a simple reference name must gain backquotes.
    pub bare_reference: bool,
}

impl NamePlace {
    /// What replaces the name here when the definition is renamed to `new`.
    #[must_use]
    pub fn replacement(&self, new: &str) -> String {
        if self.bare_reference && !is_simple_reference_name(new) {
            format!("`{new}`")
        } else {
            new.to_string()
        }
    }
}

/// Where the name stands in a use written as `form` from `start` to `end` of
/// `text` — `None` for a `.. toctree::` entry, which names a document, and
/// where the text holds no such construct there.
#[must_use]
pub fn use_name_place(
    form: &UseForm,
    text: &str,
    start: Position,
    end: Position,
) -> Option<NamePlace> {
    match form {
        UseForm::Role => role_target(&chars_between(text, start, end)),
        UseForm::Named(_) => named_reference(&chars_between(text, start, end)),
        UseForm::RelationId(id) => relation_id(text, start.line, id),
        UseForm::Name => whole(&chars_between(text, start, end)),
        UseForm::TocEntry => None,
    }
}

/// Where a definition's `name` stands in `text`, as the parser recorded it.
#[must_use]
pub fn definition_name_place(text: &str, name: NameRange) -> Option<NamePlace> {
    whole(&chars_between(text, name.start, name.end))
}

/// Every character of `text` from `start` up to `end`, with its position: a
/// line break between two lines as a `\n` at the end of the first.
fn chars_between(text: &str, start: Position, end: Position) -> Vec<(Position, char)> {
    let mut chars = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if number < start.line {
            continue;
        }
        if number > end.line {
            break;
        }
        for (column, c) in (1..).zip(line.chars().chain(std::iter::once('\n'))) {
            let at = Position::new(number, column);
            if at >= end {
                break;
            }
            if at >= start {
                chars.push((at, c));
            }
        }
    }
    chars
}

/// The place `chars[range]` takes, written as it is there.
fn place(
    chars: &[(Position, char)],
    range: std::ops::Range<usize>,
    bare_reference: bool,
) -> Option<NamePlace> {
    let (first, _) = chars.get(range.start)?;
    let (last, _) = chars.get(range.end.checked_sub(1)?)?;
    (range.start < range.end).then(|| NamePlace {
        start: *first,
        end: Position::new(last.line, last.column + 1),
        written: chars[range].iter().map(|(_, c)| c).collect(),
        bare_reference,
    })
}

/// The whole of `chars`, which is exactly a name.
fn whole(chars: &[(Position, char)]) -> Option<NamePlace> {
    place(chars, 0..chars.len(), false)
}

/// The target of the role `chars` hold: inside its backquotes, after any
/// explicit title, past a `!` or `~` marking how it is shown.
fn role_target(chars: &[(Position, char)]) -> Option<NamePlace> {
    let open = chars.iter().position(|(_, c)| *c == '`')?;
    let close = chars.iter().rposition(|(_, c)| *c == '`')?;
    if close <= open {
        return None;
    }
    let content: String = chars[open + 1..close].iter().map(|(_, c)| c).collect();
    let range = rinx_parser::role_target_range(&content);
    let mut start = open + 1 + range.start;
    let end = open + 1 + range.end;
    if start < end && matches!(chars[start].1, '!' | '~') {
        start += 1;
    }
    place(chars, start..end, false)
}

/// The name a named reference `chars` hold is written as: in `name_`, in
/// `` `name`_ ``, in the alias of `` `text <name_>`_ ``, or after the colon
/// of an alias target `.. _a: name_`.
fn named_reference(chars: &[(Position, char)]) -> Option<NamePlace> {
    let text: String = chars.iter().map(|(_, c)| c).collect();
    let mut start = 0;
    if text.starts_with(".. _") {
        // The colon the parser ends the target's own name at, as
        // `try_parse_target` finds it.
        start = 4 + chars[4..].iter().position(|(_, c)| *c == ':')? + 1;
    }
    start += blanks(&chars[start..]);
    let mut end = chars.len() - blanks_before(&chars[start..]);
    if end <= start || chars[end - 1].1 != '_' {
        return None;
    }
    end -= 1;
    if !(chars[start].1 == '`' && chars[end - 1].1 == '`' && end - start >= 2) {
        return place(chars, start..end, true);
    }
    let (start, end) = (start + 1, end - 1);
    // An embedded alias closes the backquoted text: `text <alias>`.
    let closing = end - blanks_before(&chars[start..end]);
    let opening = chars[start..end].iter().rposition(|(_, c)| *c == '<');
    let Some(opening) = opening.filter(|_| chars[closing - 1].1 == '>') else {
        return place(chars, start..end, false);
    };
    let alias_start = start + opening + 1;
    let alias_start = alias_start + blanks(&chars[alias_start..closing - 1]);
    let alias_end = closing - 1 - blanks_before(&chars[alias_start..closing - 1]);
    // An alias names a target only with its trailing `_`; without one it is
    // a URI, which names no definition.
    if alias_end <= alias_start || chars[alias_end - 1].1 != '_' {
        return None;
    }
    place(chars, alias_start..alias_end - 1, false)
}

/// How many blanks `chars` opens with.
fn blanks(chars: &[(Position, char)]) -> usize {
    chars.iter().take_while(|(_, c)| c.is_whitespace()).count()
}

/// How many blanks `chars` ends with.
fn blanks_before(chars: &[(Position, char)]) -> usize {
    chars
        .iter()
        .rev()
        .take_while(|(_, c)| c.is_whitespace())
        .count()
}

/// The id `id` among the others on the relation option on `line` of `text`.
fn relation_id(text: &str, line: u32, id: &EntityId) -> Option<NamePlace> {
    let written = text
        .lines()
        .nth(usize::try_from(line).ok()?.checked_sub(1)?)?;
    let columns = relation_id_columns(written, id)?;
    let at = |index: usize| Some(Position::new(line, u32::try_from(index + 1).ok()?));
    Some(NamePlace {
        start: at(columns.start)?,
        end: at(columns.end)?,
        written: id.as_str().to_string(),
        bare_reference: false,
    })
}

/// Whether `name` is what docutils calls a simple reference name — word
/// characters, joined by single hyphens, underscores, periods, colons or
/// plus signs — and so may be written `name_` without backquotes.
#[must_use]
pub fn is_simple_reference_name(name: &str) -> bool {
    let mut previous_joined = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            previous_joined = false;
        } else if matches!(c, '-' | '_' | '.' | ':' | '+') && !previous_joined {
            previous_joined = true;
        } else {
            return false;
        }
    }
    !name.is_empty() && !previous_joined
}

/// Why a rename is refused, said to the author as the request's error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameRefusal {
    /// A `:doc:` or a toctree entry, which names a document.
    Document,
    /// A section, which is named by its title.
    Section,
    /// A glossary term, which is its own text.
    Term,
    /// A domain object, which is named by the code it documents.
    DomainObject,
    /// A name several documents define, which leads nowhere.
    Ambiguous { name: String },
    /// An entity id composed from attributes, generated or imported.
    UnwrittenId { id: String },
    /// A definition whose name's characters are unknown.
    UnknownSite { name: String },
    /// A use in a `conf.py` project's prolog or epilog, which no file holds.
    InPrologOrEpilog { name: String },
    /// A place in a file outside every workspace folder.
    OutsideWorkspace { path: PathBuf },
    /// A file that could not be read.
    Unreadable { path: PathBuf },
    /// A place whose text no longer names the definition.
    Stale {
        path: PathBuf,
        line: u32,
        name: String,
    },
    /// A new name that cannot stand where the old one does.
    InvalidName { name: String, reason: &'static str },
    /// A new name the project already defines.
    Taken { name: String },
    /// A new entity id its type's pattern refuses.
    IdPattern { id: String, pattern: String },
}

impl fmt::Display for RenameRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document => f.write_str("a document is renamed by renaming its file"),
            Self::Section => {
                f.write_str("a section is named by its title; give it a label to rename that")
            }
            Self::Term => f.write_str("a glossary term is its own text; edit the glossary"),
            Self::DomainObject => f.write_str("a domain object is named by the code it documents"),
            Self::Ambiguous { name } => write!(
                f,
                "'{name}' is defined more than once in this project, so it names nothing"
            ),
            Self::UnwrittenId { id } => write!(
                f,
                "the id '{id}' is composed, generated or imported; write it as the entity's \
                 `:id:` to rename it"
            ),
            Self::UnknownSite { name } => write!(f, "where '{name}' is defined is not known"),
            Self::InPrologOrEpilog { name } => write!(
                f,
                "'{name}' is used in conf.py's rst_prolog or rst_epilog, which a rename does not \
                 edit"
            ),
            Self::OutsideWorkspace { path } => {
                write!(f, "{} is outside every workspace folder", path.display())
            }
            Self::Unreadable { path } => write!(f, "{} could not be read", path.display()),
            Self::Stale { path, line, name } => write!(
                f,
                "line {line} of {} no longer reads as '{name}'; save it and try again",
                path.display()
            ),
            Self::InvalidName { name, reason } => write!(f, "'{name}': {reason}"),
            Self::Taken { name } => write!(f, "'{name}' is already defined in this project"),
            Self::IdPattern { id, pattern } => {
                write!(f, "'{id}' does not match its type's id pattern '{pattern}'")
            }
        }
    }
}

#[cfg(test)]
mod tests;
