# Development Guidelines

- Name types after what they do, not what they are used for.
- Enforce invariants at the type level using opaque types with smart constructors ("parse, don't validate").
- In a build system, deserialization should validate invariants strictly and return errors on violations.
- All functions — including private and helper functions — should be thoroughly unit tested.
- Prefer a single configuration file over accumulating individual CLI flags; adding future fields requires no interface changes.
- Configuration files should contain metadata and settings, not file paths; sandboxed build systems relocate files, breaking embedded paths.
- Relative file paths work correctly for offline viewing; do not use inlining as a workaround for path computation.
- Function names should describe what the function concretely does, not just reflect its abstract role (e.g., prefer `build_project_index()` over `analyze_many()`).
- Guidelines are beliefs, preferences and style choices, not factual statements about tools or technologies.
- After every code modification session, run `cargo clippy --tests` and resolve all warnings before finishing.
- After every code modification session, run `cargo fmt`.
- Documentation builds should fail loudly if content invariants (like missing diagram images) are violated.
- Tests should always follow the Given-When-Then pattern.
- Do not commit changes unless explicitly requested by the user.
- Always add examples for all variants of a new feature to the example project.
