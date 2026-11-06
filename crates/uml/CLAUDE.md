# Notes on diagrams (`rinx_uml`)

- **Diagrams are opt-in per library** (`diagrams = True`): Bazel must declare actions before reading any document, so without the opt-in every document would pay for the pipeline. A chart (`.. entity-pie::`, `.. entity-bar::`) is drawn by the renderer itself and needs no opt-in.
- **Compilation is a site action, after indexing**, because a templated diagram asks the entity graph questions only the `ProjectIndex` can answer. Keep one path for all diagrams, so there is one population of hashes.
- The render action writes the `.puml` files it already expanded to hash. The `.puml` directory is the cache firewall in front of the JVM, so **expansion must be deterministic**.
- Generated diagrams (`.. entity-flow::`, `.. entity-sequence::`) carry a question, not generated template text, so diagnostics point at what the author wrote. They share `assemble.rs` with templated ones.
- Layout is ELK, never a host `dot`: a missing Graphviz makes PlantUML draw an error picture and exit 0, which `validate_assets` fails on as a backstop. PlantUML is pinned to JDK 21.
- `filter()`, hrefs and an entity's fields come from `rinx_index`, the same code the tables use, so a diagram and a table cannot disagree.
