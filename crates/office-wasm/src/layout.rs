//! Revision-bound layout transport and a single cache outside document history.

use std::io::{self, Write};

use office_layout::{DocumentLayout, LayoutEngine, LayoutOptions};
use serde::Serialize;

use crate::{BrowserEngine, EngineError};

const MAX_LAYOUT_REQUEST_BYTES: usize = 8 * 1024;
const MAX_LAYOUT_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct LayoutState {
    engine: Option<LayoutEngine>,
    cached: Option<CachedLayout>,
}

struct CachedLayout {
    revision: String,
    options: String,
    json: String,
}

impl LayoutState {
    pub(crate) fn invalidate(&mut self) {
        // Keep the immutable font catalog and shaping context, but release the
        // old document's potentially large geometry after a session replacement.
        self.cached = None;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LayoutView<'a> {
    session_id: &'a str,
    revision: &'a str,
    document_id: &'a str,
    layout: &'a DocumentLayout,
}

impl BrowserEngine {
    /// Returns bounded page, line, glyph and cursor geometry computed in Rust.
    ///
    /// Options are a strict JSON object; `{}` selects the pinned layout profile.
    /// If supplied, the revision must match this live session. The response is
    /// always bound to its session, document and current opaque revision token.
    /// Reading layout never creates an edit/history entry or changes selection.
    /// The font catalog initializes once on demand and survives document changes;
    /// one response is cached by revision and normalized options. Selection-only
    /// commands reuse that geometry and ordinary presentation snapshots contain
    /// no glyph outlines.
    pub fn layout_snapshot(
        &mut self,
        options_json: &str,
        expected_revision: Option<&str>,
    ) -> Result<String, EngineError> {
        let revision = self.revision();
        if expected_revision.is_some_and(|expected| expected != revision) {
            return Err(EngineError::new(
                "stale_layout",
                "The requested layout revision no longer identifies the active document.",
            ));
        }
        if options_json.len() > MAX_LAYOUT_REQUEST_BYTES {
            return Err(EngineError::new(
                "invalid_layout_options",
                "Layout options exceed the 8 KiB request limit.",
            ));
        }
        if !options_json.trim_start().starts_with('{') {
            return Err(EngineError::new(
                "invalid_layout_options",
                "Layout options must be a JSON object.",
            ));
        }
        let options: LayoutOptions = serde_json::from_str(options_json)
            .map_err(|error| EngineError::new("invalid_layout_options", error.to_string()))?;
        let normalized = serde_json::to_string(&options)
            .map_err(|error| EngineError::new("invalid_layout_options", error.to_string()))?;
        if let Some(cached) = &self.layout_state.cached
            && cached.revision == revision
            && cached.options == normalized
        {
            return Ok(cached.json.clone());
        }

        if self.layout_state.engine.is_none() {
            self.layout_state.engine = Some(
                LayoutEngine::new()
                    .map_err(|error| EngineError::new(error.code(), error.to_string()))?,
            );
        }
        let engine = self
            .layout_state
            .engine
            .as_mut()
            .expect("the layout engine was initialized above");
        let layout = engine
            .layout_document(self.session.document(), &options)
            .map_err(|error| EngineError::new(error.code(), error.to_string()))?;
        let view = LayoutView {
            session_id: &self.session_id,
            revision: &revision,
            document_id: self.session.document().id().as_str(),
            layout: &layout,
        };
        let mut output = LayoutOutput::default();
        serde_json::to_writer(&mut output, &view).map_err(|error| {
            if output.exceeded {
                EngineError::new(
                    "layout_too_large",
                    "The computed layout exceeds the 32 MiB response limit.",
                )
            } else {
                EngineError::new("layout_failed", error.to_string())
            }
        })?;
        let json =
            String::from_utf8(output.bytes).expect("the JSON serializer produces valid UTF-8");
        self.layout_state.cached = Some(CachedLayout {
            revision,
            options: normalized,
            json: json.clone(),
        });
        Ok(json)
    }
}

#[derive(Default)]
struct LayoutOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for LayoutOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > MAX_LAYOUT_RESPONSE_BYTES {
            self.exceeded = true;
            return Err(io::Error::other("layout response byte budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
