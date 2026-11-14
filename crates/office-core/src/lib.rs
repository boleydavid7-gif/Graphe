//! Interface-independent foundations for the Office Engine.
//!
//! This crate is intended to support the engine's shared document semantics and
//! invariants as those responsibilities become necessary. Editing, layout,
//! rendering, and file formats will have explicit boundaries; their eventual
//! crate organization will follow demonstrated needs.
//!
//! The core must remain independent of React, JavaScript, and browser APIs so
//! that future native applications and WebAssembly integrations can share it.
//! At this milestone, only version information is implemented.

/// Returns the version of the engine core, as declared in its Cargo manifest.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_available() {
        let version = super::version();

        assert!(!version.is_empty());
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
