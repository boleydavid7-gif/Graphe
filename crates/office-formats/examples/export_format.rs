//! Regenerates format artifacts for independent-reader compatibility checks.
//!
//! Usage: `cargo run -p office-formats --example export_format -- input.writer odt output.odt`
//! This verification utility refuses to overwrite an existing output path.

use std::{error::Error, io::Write};

use office_core::deserialize_writer_file;
use office_formats::{DocumentFormat, assess_export, export_file};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("expected input.writer, format and a new output path".into());
    }
    let format = DocumentFormat::parse(
        arguments[1]
            .to_str()
            .ok_or("format must be a canonical UTF-8 extension")?,
    )?;
    let file = deserialize_writer_file(&std::fs::read(&arguments[0])?)?;
    for warning in assess_export(format, file.document())?.warnings {
        eprintln!("{}: {}", warning.code, warning.message);
    }
    if file.original_source().is_some() {
        eprintln!(
            "retained_original: native files retain original bytes, which may include removed content; external copies contain current supported content instead."
        );
    }
    let bytes = export_file(format, &file)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments[2])?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    println!("{} bytes", bytes.len());
    Ok(())
}
