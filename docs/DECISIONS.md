# Decisions

These decisions establish the initial direction. They may be revisited when
implementation evidence justifies a change; changes should record the reason
and consequences here.

## 001: Use Rust and Cargo for the engine

**Decision:** Use stable Rust with edition 2024 and Cargo workspace management.

**Reason:** An Office engine needs predictable performance, memory safety, and
maintainable code across a long development life. Rust's ownership model and type
system help enforce invariants without requiring a garbage collector. Cargo
provides a standard build, dependency, and test workflow. Rust also supports
native compilation and WebAssembly, which aligns with browser and desktop goals.

**Tradeoffs:** Ownership and API design require care, and font, layout, and file
format behavior still need substantial engineering and testing. Rust does not
by itself provide Word compatibility or identical layout across platforms.
The stable toolchain tracks stable releases; committing `Cargo.lock` preserves
dependency resolution without freezing the compiler.

## 002: Keep the engine separate from the user interface

**Decision:** Keep core code independent of React, JavaScript, and browser APIs.
Future browser integration will use WebAssembly; React and TypeScript will be
interface technologies only.

**Reason:** Document correctness and compatibility must not depend on a particular
interface or runtime. This separation allows engine tests without a browser and
supports native desktop applications without rewriting the engine.

**Consequences:** Bindings and platform integration must have explicit boundaries.
The document model, editing operations, layout, and rendering also need distinct
responsibilities. No interface or binding layer is implemented in Step 1.

## 003: Begin with Writer

**Decision:** Start with word processing and Microsoft Word DOCX compatibility.
Spreadsheets and presentations follow later.

**Reason:** Writer provides a focused first application with concrete document
and compatibility requirements. Attempting all Office applications at once would
spread implementation and validation across different models before any one
application has a sound foundation.

**Consequences:** The initial design will be informed by Writer. Shared abstractions
will be introduced when actual cross-application requirements justify them.

## 004: Introduce structure and dependencies only when needed

**Decision:** Start with one dependency-free `office-core` crate. Do not use an
existing complete Office or word-processing engine.

**Reason:** A small foundation is easier to understand and validate. Concrete
responsibilities should determine crate boundaries and dependency choices.
Established low-level libraries remain an option as requirements arise.

**Consequences:** Future dependencies will use Cargo workspace dependency
management and require evaluation for correctness, performance, portability,
maintenance, and licensing.

## 005: Leave licensing pending

**Status:** Pending project-owner decision.

No software license has been selected. The project owner will decide whether
the engine is open-source, source-available, or proprietary. The `LICENSE` file
and package license declarations remain absent until that decision is made.
