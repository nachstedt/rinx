//! The `MiniJinja` environment a diagram's template is expanded in.
//!
//! Six functions and two names, which is the whole surface sphinx-needs'
//! `needuml` offers and very nearly the whole surface its corpus uses:
//!
//! | written | what it does |
//! |---|---|
//! | `needs` | every entity in the project, by id |
//! | `need` | the entity an `.. entity-arch::` sits inside |
//! | `need(id)` | one entity's fields |
//! | `filter(expr)` | the entities matching a filter, in id order |
//! | `flow(id)` | a clickable `PlantUML` node for one entity |
//! | `ref(id, text)` | a `PlantUML` link to one entity |
//! | `uml(id, key)` | the diagram another entity wrote, expanded here |
//! | `imports(id, rel…)` | the diagrams of everything `id` points at |
//!
//! The last two are what make an architecture diagram compositional: a
//! component writes its own picture once, and every diagram that reaches it
//! along a relation pulls that picture in rather than restating it.
//!
//! Two properties are load-bearing rather than nice. **Determinism**: every
//! iteration is over a `BTreeMap`, so the same project produces the same
//! bytes, and the bytes are hashed into the filename of the SVG the build
//! compiles. **Shared meaning**: `filter()` is `rusty_sphinx_filter` and
//! nothing else, and `flow()`/`ref()` build their hrefs with the functions the
//! renderer builds a page's own links with — a filter or a link that meant one
//! thing in a table and another in a diagram would be a bug neither crate's
//! tests could see.

use std::sync::{Arc, Mutex};

use minijinja::{Environment, Error, ErrorKind, Value};

use crate::error::UmlError;
use crate::node::{node_for, quote_safe};
use crate::snapshot::Snapshot;

/// Where a function records a structured failure on its way out.
///
/// `MiniJinja` carries its own error type, and flattening a
/// [`UmlError::UnknownEntity`] into a string on the way through would throw
/// away the diagnostic code an author needs to `.. noqa:` it. So a function
/// records the real failure here and returns a `MiniJinja` error to stop the
/// render; [`render`] then prefers what was recorded.
type FirstFailure = Arc<Mutex<Option<UmlError>>>;

/// The chain of entities an import is currently inside.
///
/// Shared rather than passed down, because the recursion runs *inside*
/// `MiniJinja`: a template calls `uml('B')`, which renders B's template, which
/// may call `uml('A')` again. Nothing on the Rust stack connects those two
/// calls, so the chain has to live beside the environment they share.
type ImportChain = Arc<Mutex<Vec<String>>>;

/// How deep an import chain may go before it is called a cycle.
///
/// A backstop, not the mechanism: [`ImportChain`] catches a genuine cycle
/// exactly, and this only bounds a chain that is long without repeating —
/// which no real architecture is, and which would otherwise expand
/// exponentially before anyone noticed.
const MAX_IMPORT_DEPTH: usize = 32;

/// Records `error` as the reason this expansion failed, keeping the first.
///
/// The first rather than the last because evaluation stops at the first
/// failure anyway — anything after it is a consequence, and reporting a
/// consequence in place of a cause is how a diagnostic sends someone to the
/// wrong line.
fn record(failure: &FirstFailure, error: UmlError) -> Error {
    let message = error.to_string();
    if let Ok(mut slot) = failure.lock()
        && slot.is_none()
    {
        *slot = Some(error);
    }
    Error::new(ErrorKind::InvalidOperation, message)
}

/// Expands `template` against `snapshot`, with `extra` and `need` bound.
///
/// # Errors
///
/// Returns the structured failure a function recorded, or a
/// [`UmlError::Template`] for a syntax or evaluation error `MiniJinja` found on
/// its own.
pub(crate) fn render(
    template: &str,
    snapshot: Snapshot,
    extra: &std::collections::BTreeMap<String, String>,
    enclosing: Option<&str>,
) -> Result<String, UmlError> {
    let snapshot = Arc::new(snapshot);
    let failure: FirstFailure = Arc::new(Mutex::new(None));
    let chain: ImportChain = Arc::new(Mutex::new(Vec::new()));

    if let Some(id) = enclosing
        && !snapshot.contains(id)
    {
        return Err(UmlError::UnknownEntity(id.to_string()));
    }

    render_with(template, &snapshot, &failure, &chain, extra, enclosing)
}

/// Renders one template in an environment sharing `failure` and `chain`.
///
/// Called again, from inside `MiniJinja`, for every `uml()` import — which is
/// why the failure slot and the import chain are parameters rather than
/// locals: an unknown entity three imports deep must still surface as itself,
/// and a cycle is only visible to something that outlives one render.
fn render_with(
    template: &str,
    snapshot: &Arc<Snapshot>,
    failure: &FirstFailure,
    chain: &ImportChain,
    extra: &std::collections::BTreeMap<String, String>,
    enclosing: Option<&str>,
) -> Result<String, UmlError> {
    let mut env = Environment::new();
    add_need(&mut env, snapshot, failure);
    add_filter(&mut env, snapshot, failure);
    add_flow(&mut env, snapshot, failure);
    add_ref(&mut env, snapshot, failure);
    add_uml(&mut env, snapshot, failure, chain);
    add_imports(&mut env, snapshot, failure, chain);

    let mut context = std::collections::BTreeMap::new();
    context.insert("needs".to_string(), snapshot.all());
    for (name, value) in extra {
        context.insert(name.clone(), Value::from(value.clone()));
    }
    // Bound last so it cannot be shadowed by an `:extra:` — an
    // `.. entity-arch::` whose `need` is not the need it sits in would draw a
    // convincing picture of the wrong thing.
    if let Some(id) = enclosing
        && let Some(entity) = snapshot.entity(id)
    {
        context.insert("need".to_string(), entity);
    }

    match env.render_str(template, Value::from(context)) {
        Ok(text) => Ok(text),
        Err(error) => Err(recorded_or_template(failure, &error)),
    }
}

/// The structured failure a function recorded, or `MiniJinja`'s own.
fn recorded_or_template(failure: &FirstFailure, error: &Error) -> UmlError {
    if let Ok(mut slot) = failure.lock()
        && let Some(recorded) = slot.take()
    {
        return recorded;
    }
    UmlError::Template {
        message: error.to_string(),
        line: error.line(),
    }
}

/// `need(id)` — one entity's fields.
fn add_need(env: &mut Environment<'_>, snapshot: &Arc<Snapshot>, failure: &FirstFailure) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    env.add_function("need", move |id: String| {
        snapshot
            .entity(&id)
            .ok_or_else(|| record(&failure, UmlError::UnknownEntity(id.clone())))
    });
}

/// `filter(expr)` — the ids matching a filter, in id order.
///
/// The expression is this build's filter language, parsed and evaluated by the
/// same crate a listing directive's `:filter:` goes through. Unlike that one it
/// is parsed *here* rather than by the parser, because the text is written
/// inside a template and only exists once the template runs — which is why an
/// unsupported construct is reported at render time for a diagram and at parse
/// time for a table.
fn add_filter(env: &mut Environment<'_>, snapshot: &Arc<Snapshot>, failure: &FirstFailure) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    env.add_function("filter", move |expression: String| {
        let parsed = rusty_sphinx_filter::parse_filter(&expression).map_err(|error| {
            record(
                &failure,
                UmlError::InvalidFilter {
                    filter: expression.clone(),
                    message: error.to_string(),
                },
            )
        })?;
        Ok::<Value, Error>(Value::from(
            snapshot
                .matching(&parsed)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        ))
    });
}

/// `flow(id)` — a clickable `PlantUML` node for one entity.
fn add_flow(env: &mut Environment<'_>, snapshot: &Arc<Snapshot>, failure: &FirstFailure) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    env.add_function("flow", move |id: String| {
        if !snapshot.contains(&id) {
            return Err(record(&failure, UmlError::UnknownEntity(id.clone())));
        }
        Ok(Value::from(node_for(&snapshot, &id)))
    });
}

/// `ref(id, text)` — a `PlantUML` link to one entity.
fn add_ref(env: &mut Environment<'_>, snapshot: &Arc<Snapshot>, failure: &FirstFailure) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    env.add_function("ref", move |id: String, text: Option<String>| {
        let Some(href) = snapshot.href(&id) else {
            return Err(record(&failure, UmlError::UnknownEntity(id.clone())));
        };
        let label = text.unwrap_or_else(|| snapshot.title(&id).to_string());
        Ok(Value::from(format!("[[{href} {}]]", quote_safe(&label))))
    });
}

/// `uml(id, key)` — the diagram another entity wrote, expanded here.
///
/// The imported template is expanded with `need` rebound to *that* entity, so
/// one architecture block written once inside a component draws that component
/// wherever it is pulled in. That is the whole of what makes `needarch`
/// compositional rather than a second way to spell `needuml`.
fn add_uml(
    env: &mut Environment<'_>,
    snapshot: &Arc<Snapshot>,
    failure: &FirstFailure,
    chain: &ImportChain,
) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    let chain = Arc::clone(chain);
    env.add_function("uml", move |id: String, key: Option<String>| {
        import_one(
            &snapshot,
            &failure,
            &chain,
            &id,
            key.as_deref().unwrap_or(""),
        )
    });
}

/// `imports(relation, ...)` — the diagrams of everything this entity points at.
///
/// Only meaningful with a `need` bound, which is why it is an
/// `.. entity-arch::` idiom: it walks the enclosing entity's own edges. The
/// results are concatenated in the order the targets were written, so the
/// generated `PlantUML` — and its hash — is stable.
fn add_imports(
    env: &mut Environment<'_>,
    snapshot: &Arc<Snapshot>,
    failure: &FirstFailure,
    chain: &ImportChain,
) {
    let snapshot = Arc::clone(snapshot);
    let failure = Arc::clone(failure);
    let chain = Arc::clone(chain);
    env.add_function(
        "imports",
        move |id: String, relations: minijinja::value::Rest<String>| {
            let mut parts = Vec::new();
            for relation in relations.iter() {
                for target in snapshot.targets(&id, relation) {
                    let text = import_one(&snapshot, &failure, &chain, &target, "")?;
                    let rendered = text.to_string();
                    if !rendered.is_empty() {
                        parts.push(rendered);
                    }
                }
            }
            Ok::<Value, Error>(Value::from(parts.join("\n")))
        },
    );
}

/// Expands one entity's stored diagram, guarding against a cycle.
///
/// An entity that stored no diagram under this key contributes nothing rather
/// than failing: that is what lets `imports('links')` pull in a relation whose
/// targets do not all carry a diagram.
fn import_one(
    snapshot: &Arc<Snapshot>,
    failure: &FirstFailure,
    chain: &ImportChain,
    id: &str,
    key: &str,
) -> Result<Value, Error> {
    if !snapshot.contains(id) {
        return Err(record(failure, UmlError::UnknownEntity(id.to_string())));
    }

    if let Some(cycle) = entered(chain, id) {
        return Err(record(failure, UmlError::RecursiveImport { chain: cycle }));
    }

    let template = snapshot.uml_template(id, key).map(str::to_string);
    let expanded = match template {
        Some(template) => render_with(
            &template,
            snapshot,
            failure,
            chain,
            &std::collections::BTreeMap::new(),
            Some(id),
        )
        .map_err(|error| record(failure, error)),
        None => Ok(String::new()),
    };
    left(chain);
    Ok(Value::from(expanded?))
}

/// Pushes `id` onto the import chain, or reports the cycle it would close.
///
/// Returns the chain *including* the repeat, so the diagnostic can show the
/// route rather than just naming the entity it ended on.
fn entered(chain: &ImportChain, id: &str) -> Option<Vec<String>> {
    let Ok(mut stack) = chain.lock() else {
        return None;
    };
    if stack.iter().any(|seen| seen == id) || stack.len() >= MAX_IMPORT_DEPTH {
        let mut cycle = stack.clone();
        cycle.push(id.to_string());
        return Some(cycle);
    }
    stack.push(id.to_string());
    None
}

/// Pops the entity an import just finished expanding.
fn left(chain: &ImportChain) {
    if let Ok(mut stack) = chain.lock() {
        stack.pop();
    }
}

#[cfg(test)]
mod tests;
