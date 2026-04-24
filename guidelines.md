# Development Guidelines

- Name types after what they do, not what they are used for.
- Enforce invariants at the type level using opaque types with smart constructors ("parse, don't validate").
- In a build system, deserialization should validate invariants strictly and return errors on violations.
