//! Prints deterministic layout JSON for native/WASM regression comparisons.

use std::{env, error::Error, io};

use office_core::load_writer_file;
use office_layout::{LayoutEngine, LayoutOptions};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let path = arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: layout_snapshot FILE.writer",
        )
    })?;
    if arguments.next().is_some() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "expected one .writer file").into(),
        );
    }
    let file = load_writer_file(path)?;
    let layout =
        LayoutEngine::new()?.layout_document(file.document(), &LayoutOptions::default())?;
    serde_json::to_writer(io::stdout().lock(), &layout)?;
    Ok(())
}
