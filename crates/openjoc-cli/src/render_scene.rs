use openjoc_render_scene::{RenderBackend, RenderRequest, inspect_sofa, render};
use std::{error::Error, io, path::PathBuf};

pub fn run_sofa(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.first().map(String::as_str) != Some("inspect") {
        return Err(usage().into());
    }
    let mut input = None;
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json = true;
                i += 1;
            }
            value if !value.starts_with('-') && input.is_none() => {
                input = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage().into()),
        }
    }
    let value = inspect_sofa(&input.ok_or_else(usage)?)?;
    if json {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("SimpleFreeFieldHRIR SOFA");
        println!("convention_version: {}", value["convention_version"]);
        println!("sample_rate_hz: {}", value["sample_rate_hz"]);
        println!("measurement_count: {}", value["measurement_count"]);
        println!(
            "expanded_max_tap_length: {}",
            value["expanded_max_tap_length"]
        );
    }
    Ok(())
}

pub fn run_render_scene(args: &[String]) -> Result<(), Box<dyn Error>> {
    let scene = args
        .first()
        .filter(|v| !v.starts_with('-'))
        .map(PathBuf::from)
        .ok_or_else(usage)?;
    let mut sofa = None;
    let mut output = None;
    let mut backend = None;
    let mut partition_size = 256_usize;
    let mut block_size = 1024_usize;
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        if flag == "--json" {
            json = true;
            i += 1;
            continue;
        }
        let val = args.get(i + 1).ok_or_else(usage)?;
        match flag.as_str() {
            "--binaural-sofa" => sofa = Some(PathBuf::from(val)),
            "--output" => output = Some(PathBuf::from(val)),
            "--backend" => backend = Some(val.as_str()),
            "--partition-size" => partition_size = val.parse()?,
            "--block-size" => block_size = val.parse()?,
            _ => return Err(usage().into()),
        }
        i += 2;
    }
    let backend = match backend.ok_or_else(usage)? {
        "direct" => RenderBackend::Direct,
        "partitioned" => RenderBackend::Partitioned { partition_size },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "backend must be direct or partitioned",
            )
            .into());
        }
    };
    let result = render(&RenderRequest {
        scene_path: scene,
        sofa_path: sofa.ok_or_else(usage)?,
        output_dir: output.ok_or_else(usage)?,
        backend,
        block_size,
    })?;
    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!(
            "render complete: {} samples, {} backend, output={}",
            result.output_sample_count, result.backend, result.output_wav
        );
    }
    Ok(())
}

fn usage() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: openjoc sofa inspect FILE [--json] | openjoc render-scene SCENE --binaural-sofa FILE --output DIR --backend direct|partitioned [--partition-size N] [--block-size N] [--json]",
    )
}
