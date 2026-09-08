// pattern: Imperative Shell

use openjoc_inspect::{InspectionOptions, format_summary, inspect_path};
use std::{
    error::Error,
    io::{self, Write},
    path::Path,
};

pub fn run(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let input = arguments
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .ok_or_else(super::usage_error)?;
    let mut options = InspectionOptions::default();
    let mut json = false;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--json" => json = true,
            "--aus" => options.aus = true,
            "--objects" => options.objects = true,
            "--emdf" => options.emdf = true,
            "--verbose" => options.verbose = true,
            "--trim-config-count" => {
                index += 1;
                options.trim_configuration_count = Some(super::parse_trim_configuration_count(
                    arguments.get(index).ok_or_else(super::usage_error)?,
                )?);
            }
            "--au" | "--au-range" => {
                index += 1;
                let value = arguments.get(index).ok_or_else(super::usage_error)?;
                let (first, last) = value.split_once(':').unwrap_or((value, value));
                let range = (first.parse::<u64>()?, last.parse::<u64>()?);
                if range.0 > range.1 {
                    return Err(super::usage_error().into());
                }
                options.au_range = Some(range);
                options.aus = true;
            }
            _ => return Err(super::usage_error().into()),
        }
        index += 1;
    }
    let report = inspect_path(Path::new(input), options)?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if json {
        serde_json::to_writer_pretty(&mut output, &report)?;
        writeln!(output)?;
    } else {
        output.write_all(format_summary(&report, options).as_bytes())?;
    }
    // A partial report is still emitted before the process returns failure.
    if !report.diagnostics.complete {
        return Err(openjoc_container::InputMediaError::InvalidDemuxedEac3(
            openjoc_eac3::Eac3Error::InvalidAccessUnitRange,
        )
        .into());
    }
    Ok(())
}
