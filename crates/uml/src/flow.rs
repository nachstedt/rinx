//! Generating a flowchart of the entity graph — what `.. entity-flow::` draws.
//!
//! The directive carries a *question*: which entities to draw, and which
//! relations between them become edges. This module answers it against the
//! project index and writes the `PlantUML` that answer looks like. Nothing an
//! author wrote reaches `PlantUML` here, which is the one thing separating this
//! from [`crate::expand`] — everything *after* the text exists is shared, so a
//! generated picture is hashed, written and compiled exactly as a written one
//! is.
//!
//! Determinism is load-bearing, as everywhere in this crate: the selection is
//! drawn from the snapshot's id order, the relations from a sorted list, and the
//! targets in the order they were written. The hash is the SVG's filename, so
//! an order that varied would recompile every diagram on every build.

use std::collections::BTreeSet;

use rusty_sphinx_ast::{EntityFlow, HashedContent};

use crate::assemble::finished;
use crate::context::UmlContext;
use crate::error::FlowError;
use crate::node::{alias, node_for, quote_safe};
use crate::snapshot::Snapshot;

/// Draws one flowchart, returning the `PlantUML` text to compile.
///
/// # Errors
///
/// Returns [`FlowError::EmptyResult`] when the filter matched no entity, and
/// [`FlowError::UnknownConfig`] when `:config:` names a preamble the site does
/// not declare. Nothing else can fail here: the filter and the relation names
/// were checked while parsing, where the option line's position was still in
/// hand.
pub fn build_flow(flow: &EntityFlow, ctx: &UmlContext<'_>) -> Result<HashedContent, FlowError> {
    let snapshot = Snapshot::build(ctx.index, ctx.schema, ctx.doc_path);
    let selected = selected_ids(flow, &snapshot);
    if selected.is_empty() {
        return Err(FlowError::EmptyResult);
    }

    let mut lines = Vec::new();
    lines.extend(flow.direction.statement().map(str::to_string));
    lines.extend(selected.iter().map(|id| node_for(&snapshot, id)));
    lines.extend(edges(flow, &snapshot, &selected, ctx));

    finished(&lines.join("\n"), flow.config.as_deref(), ctx).map_err(FlowError::from)
}

/// The entities to draw, in id order.
///
/// An absent `:filter:` draws the whole project, which is what omitting it
/// means in sphinx-needs too — and what a filter that failed to parse falls
/// back to, since the parser already reported it and an empty picture would
/// say the same thing again, less usefully.
fn selected_ids(flow: &EntityFlow, snapshot: &Snapshot) -> Vec<String> {
    let ids = match &flow.filter {
        Some(filter) => snapshot.matching(filter),
        None => snapshot.ids(),
    };
    ids.into_iter().cloned().collect()
}

/// Every edge between two selected entities, as `PlantUML` arrows.
///
/// Edges whose target was not selected are left out rather than drawn to a
/// node that is not there: a filter selects a *subgraph*, and `PlantUML` would
/// otherwise invent an unlabelled box for every entity the filter excluded —
/// which is the one thing a filtered picture is supposed to avoid.
fn edges(
    flow: &EntityFlow,
    snapshot: &Snapshot,
    selected: &[String],
    ctx: &UmlContext<'_>,
) -> Vec<String> {
    let drawn: BTreeSet<&String> = selected.iter().collect();
    let relations = flow
        .relations
        .clone()
        .unwrap_or_else(|| ctx.schema.relation_names());

    let mut edges = Vec::new();
    for source in selected {
        for relation in &relations {
            for target in snapshot.targets(source, relation) {
                if !drawn.contains(&target) {
                    continue;
                }
                edges.push(arrow(source, &target, relation, flow, ctx));
            }
        }
    }
    edges
}

/// One edge, labelled with its relation when `:show-link-names:` asked for it.
///
/// The label is the schema's own `label` for the relation, falling back to its
/// name — the very text a rendered entity shows above its outgoing links, so a
/// picture and the page it sits on call one relation one thing.
fn arrow(
    source: &str,
    target: &str,
    relation: &str,
    flow: &EntityFlow,
    ctx: &UmlContext<'_>,
) -> String {
    let arrow = format!("{} --> {}", alias(source), alias(target));
    if !flow.show_link_names {
        return arrow;
    }
    let label = ctx
        .schema
        .relation(relation)
        .map_or(relation, |spec| spec.display_label());
    format!("{arrow} : {}", quote_safe(label))
}

#[cfg(test)]
mod tests;
