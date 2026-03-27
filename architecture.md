# Rusty-Sphinx: Architecture & Design Principles

This document outlines the core architecture and key principles for `rusty-sphinx`, a high-performance, resilient, and Bazel-friendly re-implementation of the Sphinx documentation framework in Rust.

---

## 1. Key Principles

1. **Performance-First**: Leverage Rust's zero-cost abstractions, efficient memory management (e.g., string interning or zero-copy parsing), and optimized data structures.
2. **Deterministic and Modular**: To be truly Bazel-friendly, the generation process must be deterministic and split into granular, cacheable steps with explicit inputs and outputs.
3. **Resilience and Error-Recovery**: For LSP and live-preview support, the parser must be fault-tolerant. Syntax errors should not abort the process; instead, they should produce "Error Nodes" in the Abstract Syntax Tree (AST), allowing the rest of the document to be parsed and rendered.
4. **Library-First Design**: The core logic must be an API-agnostic rust library (`rusty_sphinx_core`). The Bazel worker and LSP are simply frontends wrapping this core.

---

## 2. High-Level Architecture

The system is divided into several crates/modules to ensure a clean separation of concerns:

- `rusty_sphinx_parser`: Handles lexing and parsing of reStructuredText (RST) into an AST.
- `rusty_sphinx_ast`: Defines the AST nodes, types, and representation.
- `rusty_sphinx_analyzer`: Manages cross-references, indexing, TOC generation, and project-wide analysis.
- `rusty_sphinx_renderer`: Converts the resolved AST into the final output formats (HTML).
- `rusty_sphinx_worker`: A unified binary capable of acting as a **Bazel Persistent Worker**. Instead of spawning thousands of short-lived separate processes (`parse`, `index`, `render`), Bazel runs a single persistent background process. This allows `rusty-sphinx` to drastically cut OS process startup times and reuse memory allocations (e.g., string interning pools) across sequential parse actions, achieving massive performance gains.
- `rusty_sphinx_lsp`: The Language Server implementing the LSP protocol for editor integration.

---

## 3. Addressing the Core Requirements

### 3.1. Fast HTML Generation
- **Parallelism Management**: Parsing and rendering individual files is embarrassingly parallel. 
  - **In a Bazel context**: Parallelism is handled entirely by Bazel spawning multiple `rusty-sphinx-worker` processes. There is no standalone multi-threaded build orchestrator.
  - **In an LSP context**: `rayon` can be used to drastically speed up the initial workspace indexing of all `.rst` files.
- **Zero-copy/Arena Allocation**: By using bump allocators (e.g., `bumpalo`) or lifetimes tied to the input source file, AST generation can avoid excessive heap allocations, making it blazingly fast.

### 3.2. Bazel Compatibility (Maximized Caching)
To maximize Bazel\'s caching capabilities, the compilation pipeline must be broken down into discrete phases. A traditional Sphinx build is highly stateful; `rusty-sphinx` will use a **Multi-Phase Compilation Model**:

1. **Phase 1: Parse (1:1 Cacheable)**
   - **Input**: `page.rst`
   - **Output**: `page.ast` (A serialized binary or robust JSON representation of the local AST and local reference requirements).
   - *Bazel Benefit*: If one file changes, only that file is re-parsed.
2. **Phase 2: Indexing / Link Resolution (N:1)**
   - **Input**: All `*.ast` files.
   - **Output**: `project.index` (A global symbol table containing targets, TOC trees, and cross-reference maps).
   - *Bazel Benefit*: This step evaluates project-wide state but avoids re-parsing text.
3. **Phase 3: Render (1:1 Cacheable)**
   - **Input**: `page.ast` + `project.index`
   - **Output**: `page.html`
   - *Bazel Benefit*: Each compilation to HTML only depends on its own AST and the global index. If `A.rst` changes, `B.html` may only need re-rendering if the global index changed in a way that affects `B`.

### 3.3. Live Preview & Fault Tolerance (VSCode)
To support a live preview that renders even when the document is incomplete or contains errors:
- **Error-Resilient Parser**: The parser should implement recovery strategies. If a directive is malformed, the parser captures the malformed text as an `ErrorNode` or `RawTextNode` and continues parsing the subsequent blocks.
- **Incremental Rendering**: In a live preview, `rusty-sphinx` can skip the global indexing phase or use a stale index, simply converting the in-memory AST directly into HTML on every keystroke. This guarantees a sub-millisecond turn-around for visual updates.

### 3.4. Language Server (LSP) Derivation
A traditional compiler drops state after finishing. An LSP needs to maintain state and perform incremental updates. 
- **Query-Based Architecture**: Consider using a query-based compiler framework like **`salsa`** (used by `rust-analyzer`).
- This allows the core library to cache intermediate results (like \"get AST for file\", \"get resolved links\"). When the user edits a file in VSCode, only the queries dependent on that file\'s text are invalidated and recomputed.
- **Diagnostics**: Instead of using `Result<T, E>` to short-circuit upon finding an error, functions will return `(Output, Vec<Diagnostic>)`. This ensures that the LSP can underline multiple errors in a document without halting analysis at the first mistake.

---

## 4. Recommended Data Flow

```mermaid
graph TD;
    %% Phase 1: Local Parse
    RstFile[File.rst] -->|rusty_sphinx_parser| AST[Local AST]
    RstFile -->|Lexer Errors| Diag[Diagnostics]
    
    %% Phase 2: Global Assembly
    AST -->|Extract targets/links| Indexer[ rusty_sphinx_analyzer ]
    OtherASTs[Other Local ASTs] --> Indexer
    Indexer --> GlobalIndex[Project Index]
    Indexer -->|Missing Link Errors| Diag
    
    %% Phase 3: Render
    AST --> Renderer[rusty_sphinx_renderer]
    GlobalIndex --> Renderer
    Renderer --> HTML[Output.html]
    Renderer -->|Render Warnings| Diag
    
    %% LSP Flow
    Editor[VSCode] -.->|File Changes| RstFile
    Editor -.->|Hover/Go-to-def| Indexer
    Diag -.-> Editor
```

## 5. Next Steps for Implementation
1. **Repository Setup**: Initialize a Cargo workspace with the core crates (`ast`, `parser`, `analyzer`, `renderer`, `worker`, `lsp`).
2. **Prototyping the AST**: Define the data structure for the AST that can handle both valid RST elements and represent syntax errors organically.
3. **Drafting the Parser**: Build a basic lexer/parser pipeline capable of parsing a subset of RST (e.g., headings, paragraphs, bold/italics) and successfully outputting an `ErrorNode` for invalid syntax without panicking.
4. **Bazel Rules Draft**: Create a simple `BUILD.bazel` to test the phase 1 (parse) -> phase 2 (index) -> phase 3 (render) workflow.
