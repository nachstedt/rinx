//! Generating a sequence diagram of the entity graph — what
//! `.. entity-sequence::` draws.
//!
//! The walk is sphinx-needs' `needsequence`, ported for its observable
//! outcome: from a *sender*, each of the named relations leads to *message*
//! entities, and the same relations lead on from each message to its
//! *receivers*. Every hop is one arrow, `sender -> receiver : message title`,
//! and every receiver not yet seen becomes a sender in turn, depth first. A
//! sender is declared as a participant just before its first message, so the
//! lifelines appear in the order the walk reaches them.
//!
//! Three deliberate departures, all recorded in ADR-020:
//!
//! - **One visited set across every start.** sphinx-needs starts each
//!   `:start:` entry with an empty set, so a second start re-walks what the
//!   first already drew — declaring participants twice and drawing messages
//!   twice. Here the second start continues where the first left off.
//! - **Nothing crashes.** An unknown start is reported and skipped, and a
//!   relation target no document declares is left out — it is already a
//!   broken link wherever it was written.
//! - **Every lifeline has a title.** sphinx-needs declares only senders, so a
//!   receiver that never sends is drawn by `PlantUML` under its raw id. Here
//!   every entity a drawn arrow touches is declared.
//!
//! Determinism holds as everywhere in this crate: relations are followed in
//! the order written and targets in the order *they* were written, so an
//! unchanged graph hashes the same and its compile action stays cached.

use std::collections::BTreeSet;

use rusty_sphinx_ast::{EntitySequence, HashedContent};

use crate::assemble::finished;
use crate::context::UmlContext;
use crate::error::SequenceError;
use crate::node::{alias, quote_safe};
use crate::snapshot::Snapshot;

/// What a sequence diagram's walk produced: the text to compile, when there
/// is any, and everything worth reporting about how it came about.
///
/// Not a `Result`, because the two are not exclusive: an unknown start or a
/// truncated walk still draws a picture of everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceDrawing {
    /// The finished `PlantUML`, or `None` when nothing could be drawn.
    pub content: Option<HashedContent>,
    /// What to report, in the order it was found.
    pub problems: Vec<SequenceError>,
}

impl SequenceDrawing {
    /// How many of how many messages were drawn, when `:max-items:` cut the
    /// walk short — what the page's notice under the picture says.
    #[must_use]
    pub fn truncation(&self) -> Option<(usize, usize)> {
        self.problems.iter().find_map(|problem| match problem {
            SequenceError::Truncated { shown, total } => Some((*shown, *total)),
            _ => None,
        })
    }
}

/// Walks one sequence diagram, returning the `PlantUML` text to compile.
#[must_use]
pub fn build_sequence(sequence: &EntitySequence, ctx: &UmlContext<'_>) -> SequenceDrawing {
    let snapshot = Snapshot::build(ctx.index, ctx.schema, ctx.doc_path);
    let mut walk = Walk::new(sequence, &snapshot);
    let mut problems = Vec::new();

    let mut any_start = false;
    for start in sequence.start.as_slice() {
        if snapshot.contains(start.as_str()) {
            any_start = true;
            walk.start_at(start.as_str());
        } else {
            problems.push(SequenceError::UnknownStart(start.to_string()));
        }
    }

    if walk.arrows.is_empty() {
        // With no start known there was never a walk to come up empty, and
        // the unknown starts already say why nothing is drawn.
        if any_start {
            problems.push(SequenceError::EmptyResult);
        }
        return SequenceDrawing {
            content: None,
            problems,
        };
    }
    if walk.shown() < walk.total {
        problems.push(SequenceError::Truncated {
            shown: walk.shown(),
            total: walk.total,
        });
    }

    let content = match finished(&walk.text(), sequence.config.as_deref(), ctx) {
        Ok(content) => Some(content),
        Err(error) => {
            problems.push(error.into());
            None
        }
    };
    SequenceDrawing { content, problems }
}

/// The walk's state: what has been visited, and what has been drawn.
struct Walk<'a> {
    sequence: &'a EntitySequence,
    snapshot: &'a Snapshot,
    /// Every entity already visited as a sender — shared by every start.
    visited: BTreeSet<String>,
    /// The senders declared while `:max-items:` still had room, in the order
    /// the walk reached them.
    declared: Vec<String>,
    /// Every entity a drawn arrow touches, in the order first touched.
    drawn: Vec<String>,
    /// The message arrows, in the order the walk drew them.
    arrows: Vec<String>,
    /// How many messages the walk would have drawn with no limit.
    total: usize,
}

impl<'a> Walk<'a> {
    const fn new(sequence: &'a EntitySequence, snapshot: &'a Snapshot) -> Self {
        Self {
            sequence,
            snapshot,
            visited: BTreeSet::new(),
            declared: Vec::new(),
            drawn: Vec::new(),
            arrows: Vec::new(),
            total: 0,
        }
    }

    /// Whether `:max-items:` leaves room for another message.
    fn has_room(&self) -> bool {
        self.sequence
            .max_items
            .is_none_or(|limit| self.arrows.len() < limit.get() as usize)
    }

    /// Walks from one `:start:` entry, unless an earlier start already reached
    /// it — the visited set is shared, so its messages are already drawn.
    fn start_at(&mut self, start: &str) {
        if !self.visited.contains(start) {
            self.visit(start);
        }
    }

    /// Walks every message `sender` sends, and on into each new receiver.
    fn visit(&mut self, sender: &str) {
        for message in self.outgoing(sender) {
            self.declare(sender);
            for receiver in self.outgoing(&message) {
                if !self.keeps(&receiver) {
                    continue;
                }
                self.count_message(sender, &receiver, &message);
                if !self.visited.contains(&receiver) {
                    self.visit(&receiver);
                }
            }
        }
    }

    /// Every entity `id` points at along the diagram's relations, in the
    /// order the relations were named and the targets written, leaving out an
    /// id no document declares.
    fn outgoing(&self, id: &str) -> Vec<String> {
        self.sequence
            .relations
            .as_slice()
            .iter()
            .flat_map(|relation| self.snapshot.targets(id, relation))
            .filter(|target| self.snapshot.contains(target))
            .collect()
    }

    /// Whether the `:filter:`, when there is one, keeps `receiver`.
    fn keeps(&self, receiver: &str) -> bool {
        self.sequence
            .filter
            .as_ref()
            .is_none_or(|filter| self.snapshot.matches(receiver, filter))
    }

    /// Declares `sender` as a participant the first time it sends.
    ///
    /// Marked visited even when `:max-items:` leaves no room to declare it, so
    /// the walk's shape — and so the total it reports — does not depend on
    /// the limit; a participant only a drawn arrow still needs is declared at
    /// the end, by [`Self::text`].
    fn declare(&mut self, sender: &str) {
        if !self.visited.insert(sender.to_string()) {
            return;
        }
        if self.has_room() {
            self.declared.push(sender.to_string());
        }
    }

    /// Counts one message, drawing it while `:max-items:` leaves room.
    fn count_message(&mut self, sender: &str, receiver: &str, message: &str) {
        self.total += 1;
        if !self.has_room() {
            return;
        }
        for id in [sender, receiver] {
            if !self.drawn.iter().any(|drawn| drawn == id) {
                self.drawn.push(id.to_string());
            }
        }
        self.arrows.push(format!(
            "{} -> {} : {}",
            alias(sender),
            alias(receiver),
            quote_safe(self.snapshot.title(message))
        ));
    }

    /// How many messages were drawn.
    fn shown(&self) -> usize {
        self.arrows.len()
    }

    /// One participant's declaration: its title, its alias, and a link to its
    /// anchor, so a lifeline is as clickable as a flowchart's node.
    fn participant(&self, id: &str) -> String {
        let link = self
            .snapshot
            .href(id)
            .map_or_else(String::new, |href| format!(" [[{href}]]"));
        format!(
            "participant \"{}\" as {}{link}",
            quote_safe(self.snapshot.title(id)),
            alias(id)
        )
    }

    /// The diagram text: participants first, then the messages.
    ///
    /// The senders declared during the walk come first, in walk order; after
    /// them, every other entity a drawn arrow touches. That covers two cases
    /// sphinx-needs leaves to `PlantUML`, which invents a lifeline labelled
    /// with the raw id: a receiver that never sends anything, and the
    /// receiver of the last message `:max-items:` allowed, which reaches its
    /// own declaration with no room left. Undeclared lifelines are created
    /// after every declared one, so declaring them here moves nothing.
    fn text(&self) -> String {
        let late = self.drawn.iter().filter(|id| !self.declared.contains(id));
        self.declared
            .iter()
            .chain(late)
            .map(|id| self.participant(id))
            .chain(self.arrows.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests;
