//! Exports a native fixture through the same PDF adapter used by Writer.
//!
//! Usage: `cargo run -p office-formats --example export_pdf -- input.writer output.pdf`

use std::{error::Error, path::PathBuf};

use office_formats::{DocumentFormat, assess_export, export_document};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err("expected input.writer and output.pdf paths".into());
    }
    let source = office_core::deserialize_writer_file(&std::fs::read(&arguments[0])?)?;
    let report = assess_export(DocumentFormat::Pdf, source.document())?;
    for warning in &report.warnings {
        eprintln!("{}: {}", warning.code, warning.message);
    }
    let bytes = export_document(DocumentFormat::Pdf, source.document())?;
    std::fs::write(PathBuf::from(&arguments[1]), bytes)?;
    Ok(())
}
