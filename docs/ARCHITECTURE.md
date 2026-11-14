# Architecture

This document records long-term principles. Step 1 implements only the repository
foundation and a minimal engine library; the boundaries below describe future
work, not existing subsystems.

## Independent engine

The engine must remain independent of its interface. Core code is Rust and must
not depend on React, JavaScript, browser APIs, or a particular application shell.
An interface translates user intent into engine operations and presents engine
results.

Browser integration will use WebAssembly. Any browser or JavaScript bindings
belong at the integration boundary rather than in the core. Native desktop
applications should be possible without rewriting the engine.

## Separate document, editing, layout, and rendering concerns

The document model describes document content and semantics. Editing operations
change that model. Layout determines the placement of content, and rendering
draws the resulting layout. These responsibilities should have clear boundaries
so that display or editing choices do not define the document's meaning.

Document layout must eventually be platform-independent. Typography, font
handling, measurement, and pagination will require explicit policies and
verification so that host-specific behavior does not silently determine results.
The detailed interfaces and algorithms remain future decisions.

## Compatibility as a primary requirement

Microsoft Word DOCX compatibility is a primary requirement. Future model,
serialization, and layout decisions must account for faithful import, export,
and document behavior. Compatibility will be developed incrementally and checked
against documented expectations and fixtures as those capabilities are added.

The project will build an independent engine rather than embed an existing
complete Office or word-processing engine. Established low-level open-source
libraries may be used when appropriate, after evaluating their behavior,
portability, maintenance, and licensing against project needs.

## Incremental workspace

`office-core` is the only crate in the initial Cargo workspace. It currently
exposes an engine version and has no external dependencies. Future shared
dependencies will be declared in `[workspace.dependencies]` and inherited by
member crates. New crates should also inherit the workspace package settings and
lints. Additional crates will be introduced only when concrete responsibilities
require them; conceptual boundaries do not require empty crates or modules in
advance.

Writer is the first application. Word processing and DOCX compatibility establish
the initial requirements. Spreadsheets and presentations follow later without
forcing their models or application code into this foundation.

Correctness, performance, compatibility, maintainability, and architectural
stability guide development. Changes should keep responsibilities explicit and
be validated at the scope they introduce.
