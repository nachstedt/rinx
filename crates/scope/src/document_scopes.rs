//! The scope at every point of one document where a phase needs it, computed
//! once in memory.
//!
//! Which scope a definition is qualified in and a reference resolved in is a
//! pure function of the document: `.. py:module::`, `.. currentmodule::`,
//! class nesting, `:module:`, the `c:namespace` family and `.. program::` are
//! all written in it, and every document starts from the empty scope. So
//! instead of each phase replaying the rules in its own walk — the analyzer
//! to build index keys, the renderer to write the anchor `id`s that must equal
//! them — one walk applies them and every phase reads the answer, in whatever
//! order it visits the document.
//!
//! Nothing here is ever written to a `.ast` file: the table borrows the
//! document it was computed from, so it cannot outlive it or disagree with it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;

use rinx_ast::{Child, Directive, DomainObjectBody, InlineNode, Node, for_each_child};

use crate::{DefinitionNames, Scope};

/// The names of every domain-object definition in a document, and the scope
/// in force at every reference that resolves against one.
///
/// Looked up by the node itself — its address within the borrowed document,
/// which the borrow keeps from moving or changing while the table exists.
#[derive(Debug)]
pub struct DocumentScopes<'a> {
    definitions: HashMap<usize, DefinitionNames>,
    /// Each reference's scope, as an index into `scopes`.
    references: HashMap<usize, usize>,
    /// The distinct scopes references were found in, in document order. A
    /// scope is stored again only when it changed since the last reference,
    /// so a page holds a handful.
    scopes: Vec<Scope>,
    /// What a lookup that misses answers with.
    empty: Scope,
    document: PhantomData<&'a [Node]>,
}

impl<'a> DocumentScopes<'a> {
    /// The scopes of the document whose top-level nodes are `nodes`.
    #[must_use]
    pub fn of(nodes: &'a [Node]) -> Self {
        let mut walk = Walk::default();
        walk.nodes(nodes);
        Self {
            definitions: walk.definitions,
            references: walk.references,
            scopes: walk.scopes,
            empty: Scope::default(),
            document: PhantomData,
        }
    }

    /// The names the definition `obj` is qualified to.
    ///
    /// `obj` must be a node of the document this table was computed from;
    /// any other node is a caller's bug, which panics in a debug build and is
    /// qualified against the empty scope in a release one, so a page still
    /// renders.
    #[must_use]
    pub fn definition(&self, obj: &DomainObjectBody) -> Cow<'_, DefinitionNames> {
        if let Some(names) = self.definitions.get(&address(obj)) {
            return Cow::Borrowed(names);
        }
        debug_assert!(false, "a definition outside the document: {obj:?}");
        let mut scope = Scope::default();
        let (names, entered) = scope.enter_definition(obj);
        scope.leave(entered);
        Cow::Owned(names)
    }

    /// The scope in force where `reference` — a domain-object, `:any:` or
    /// `:option:` reference — is written.
    ///
    /// `reference` must be a node of the document this table was computed
    /// from; any other node is a caller's bug, which panics in a debug build
    /// and reads the empty scope in a release one.
    #[must_use]
    pub fn scope_at(&self, reference: &InlineNode) -> &Scope {
        if let Some(&index) = self.references.get(&address(reference)) {
            return &self.scopes[index];
        }
        debug_assert!(false, "a reference outside the document: {reference:?}");
        &self.empty
    }
}

/// The key a node is looked up by: its address, which the table's borrow of
/// the document keeps stable. A `usize` rather than a pointer, so the table
/// stays `Send`.
fn address<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

/// The one walk over a document, applying the scope rules in document order.
#[derive(Default)]
struct Walk {
    scope: Scope,
    definitions: HashMap<usize, DefinitionNames>,
    references: HashMap<usize, usize>,
    scopes: Vec<Scope>,
}

impl Walk {
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Directive(Directive::DomainObject(obj)) => {
                    let (names, entered) = self.scope.enter_definition(obj);
                    self.definitions.insert(address(obj), names);
                    self.children(node);
                    self.scope.leave(entered);
                }
                Node::Directive(directive) => {
                    self.scope.apply_scope_directive(directive);
                    self.children(node);
                }
                _ => self.children(node),
            }
        }
    }

    /// Walks what `node` holds, in document order.
    fn children(&mut self, node: &Node) {
        for_each_child(node, &mut |child| match child {
            Child::Nodes(nodes) => self.nodes(nodes),
            Child::Inline(inlines) => self.inlines(inlines),
        });
    }

    /// Records the current scope for every reference in `inlines` that
    /// resolves against one.
    fn inlines(&mut self, inlines: &[InlineNode]) {
        for inline in inlines {
            if matches!(
                inline,
                InlineNode::DomainObjectReference { .. }
                    | InlineNode::AnyReference { .. }
                    | InlineNode::OptionReference { .. }
            ) {
                if self.scopes.last() != Some(&self.scope) {
                    self.scopes.push(self.scope.clone());
                }
                self.references
                    .insert(address(inline), self.scopes.len() - 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Document, for_each_inline_list, walk_nodes};

    /// Every domain object in `document`, in document order.
    fn objects(document: &Document) -> Vec<&DomainObjectBody> {
        let mut found = Vec::new();
        walk_nodes(&document.nodes, &mut |node| {
            if let Node::Directive(Directive::DomainObject(obj)) = node {
                found.push(obj);
            }
        });
        found
    }

    /// Whether `inline` is a reference that resolves against a scope.
    fn is_scoped_reference(inline: &InlineNode) -> bool {
        matches!(
            inline,
            InlineNode::DomainObjectReference { .. }
                | InlineNode::AnyReference { .. }
                | InlineNode::OptionReference { .. }
        )
    }

    /// Every reference in `document` that resolves against a scope, in
    /// document order.
    fn references(document: &Document) -> Vec<&InlineNode> {
        let mut found = Vec::new();
        walk_nodes(&document.nodes, &mut |node| {
            for_each_child(node, &mut |child| {
                if let Child::Inline(inlines) = child {
                    found.extend(inlines.iter().filter(|inline| is_scoped_reference(inline)));
                }
            });
        });
        found
    }

    /// How many references resolving against a scope
    /// [`for_each_inline_list`] — the walk the parser's whole-document passes
    /// use — finds in `document`, which [`references`] must agree with.
    fn references_inline_lists_hold(document: &Document) -> usize {
        let mut count = 0;
        for_each_inline_list(&document.nodes, &mut |inlines| {
            count += inlines
                .iter()
                .filter(|inline| is_scoped_reference(inline))
                .count();
        });
        count
    }

    /// The primary name of each domain object in `document`.
    fn primary_names(document: &Document) -> Vec<String> {
        let scopes = DocumentScopes::of(&document.nodes);
        objects(document)
            .into_iter()
            .map(|obj| match &*scopes.definition(obj) {
                DefinitionNames::Object(names) => names.first().clone(),
                DefinitionNames::Options(lines) => lines[0][0].clone(),
            })
            .collect()
    }

    /// What `name` qualifies to in the scope of each reference in `document`.
    fn qualified_at_references(document: &Document, name: &str) -> Vec<String> {
        let scopes = DocumentScopes::of(&document.nodes);
        references(document)
            .into_iter()
            .map(|reference| {
                scopes
                    .scope_at(reference)
                    .python
                    .qualify(name)
                    .qualified_name
            })
            .collect()
    }

    #[test]
    fn test_of_qualifies_a_method_under_its_class_and_module() {
        // Given
        let document = rinx_parser::parse(
            "test.rst",
            ".. py:module:: pkg\n\n.. py:class:: Widget\n\n   .. py:method:: run()\n",
        );

        // When
        let names = primary_names(&document);

        // Then
        assert_eq!(names, ["pkg", "pkg.Widget", "pkg.Widget.run"]);
    }

    #[test]
    fn test_of_keeps_a_module_current_after_the_container_it_was_set_in() {
        // Given — a `py:module` is document-order state, so leaving the note
        // it is written in does not end it
        let document = rinx_parser::parse(
            "test.rst",
            ".. note::\n\n   .. py:module:: pkg\n\n.. py:function:: run()\n",
        );

        // When
        let names = primary_names(&document);

        // Then
        assert_eq!(names, ["pkg", "pkg.run"]);
    }

    #[test]
    fn test_scope_at_sees_the_class_inside_its_body_and_not_after_it() {
        // Given
        let document = rinx_parser::parse(
            "test.rst",
            ".. py:module:: pkg\n\n\
             .. py:class:: Widget\n\n   Calls :py:meth:`run`.\n\n\
             After :py:meth:`run`.\n",
        );

        // When
        let qualified = qualified_at_references(&document, "f");

        // Then
        assert_eq!(qualified, ["pkg.Widget.f", "pkg.f"]);
    }

    #[test]
    fn test_scope_at_sees_a_module_option_only_inside_its_object() {
        // Given
        let document = rinx_parser::parse(
            "test.rst",
            ".. py:module:: pkg\n\n\
             .. py:function:: run()\n   :module: other\n\n   See :py:func:`helper`.\n\n\
             See :py:func:`helper`.\n",
        );

        // When
        let qualified = qualified_at_references(&document, "f");

        // Then
        assert_eq!(qualified, ["other.f", "pkg.f"]);
    }

    #[test]
    fn test_scope_at_applies_a_scope_change_before_the_attribution_it_precedes() {
        // Given — the module is set in a block quote's content, and the
        // reference is in its attribution, which is read after it
        let document = rinx_parser::parse(
            "test.rst",
            "Intro.\n\n   .. currentmodule:: pkg\n\n   Quoted.\n\n   -- :py:func:`author`\n",
        );

        // When
        let qualified = qualified_at_references(&document, "f");

        // Then
        assert_eq!(qualified, ["pkg.f"]);
    }

    #[test]
    fn test_scope_at_sees_the_program_an_option_reference_is_written_under() {
        // Given
        let document = rinx_parser::parse(
            "test.rst",
            ".. program:: tool\n\n.. option:: -v\n\nUse :option:`-v`.\n",
        );
        let scopes = DocumentScopes::of(&document.nodes);

        // When
        let program = scopes.scope_at(references(&document)[0]).program.current();

        // Then
        assert_eq!(program, Some("tool"));
        assert_eq!(primary_names(&document), ["tool.-v"]);
    }

    #[test]
    fn test_of_qualifies_a_c_function_in_a_pushed_namespace() {
        // Given
        let document = rinx_parser::parse(
            "test.rst",
            ".. c:namespace-push:: geometry\n\n.. c:function:: int area(void)\n\n\
             .. c:namespace-pop::\n\n.. c:function:: int volume(void)\n",
        );

        // When
        let names = primary_names(&document);

        // Then
        assert_eq!(names, ["geometry.area", "volume"]);
    }

    #[test]
    fn test_of_stores_a_scope_again_only_when_it_changed() {
        // Given — two references in one scope, a third in another
        let document = rinx_parser::parse(
            "test.rst",
            ":py:func:`a` and :py:func:`b`.\n\n.. currentmodule:: pkg\n\n:py:func:`c`.\n",
        );

        // When
        let scopes = DocumentScopes::of(&document.nodes);

        // Then
        assert_eq!(scopes.references.len(), 3);
        assert_eq!(scopes.scopes.len(), 2);
    }

    #[test]
    fn test_of_reaches_every_definition_and_reference_in_every_container() {
        // Given — a domain object and a reference in every kind of container
        // that holds content
        let document = rinx_parser::parse(
            "test.rst",
            "\
Title :py:func:`in_heading`
===========================

- :py:func:`in_bullet`

#. :py:func:`in_enumerated`

term :py:func:`in_term`
   :py:func:`in_definition`

-a  :py:func:`in_option_list`

| :py:func:`in_line`
|    :py:func:`in_nested_line`

   :py:func:`in_quote`

   -- :py:func:`in_attribution`

+-------------------------+
| :py:func:`in_grid_table`|
+-------------------------+

.. note::

   .. py:function:: in_note()

.. seealso:: :py:func:`in_seealso`

.. versionadded:: 1.0
   :py:func:`in_versionadded`

.. dropdown:: :py:func:`in_dropdown_title`

   :py:func:`in_dropdown_body`

.. grid:: 2

   .. grid-item::

      :py:func:`in_grid_item`

.. button-link:: https://example.com

   :py:func:`in_button`

.. glossary::

   term
      :py:func:`in_glossary`

.. list-table::

   * - :py:func:`in_list_table`

.. table::

   ===  ===================
   a    :py:func:`in_table`
   ===  ===================

.. figure:: picture.png

   :py:func:`in_caption`

   :py:func:`in_legend`

.. py:class:: Outer

   .. py:method:: in_class()

   :py:func:`in_class_body`
",
        );

        // When
        let scopes = DocumentScopes::of(&document.nodes);

        // Then — nothing a phase can reach is missing from the table
        let objects = objects(&document);
        let references = references(&document);
        assert_eq!(objects.len(), 3);
        assert_eq!(references.len(), 23);
        assert_eq!(references.len(), references_inline_lists_hold(&document));
        assert_eq!(scopes.definitions.len(), objects.len());
        assert_eq!(scopes.references.len(), references.len());
        for reference in references {
            assert!(scopes.references.contains_key(&address(reference)));
        }
    }

    #[test]
    #[should_panic(expected = "a reference outside the document")]
    #[cfg(debug_assertions)]
    fn test_scope_at_rejects_a_reference_from_another_document_in_a_debug_build() {
        // Given
        let document = rinx_parser::parse("test.rst", "Text.\n");
        let other = rinx_parser::parse("other.rst", ":py:func:`f`\n");
        let scopes = DocumentScopes::of(&document.nodes);

        // When / Then
        let _ = scopes.scope_at(references(&other)[0]);
    }
}
