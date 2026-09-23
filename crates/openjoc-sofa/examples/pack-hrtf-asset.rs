// pattern: Imperative Shell

use std::{env, fs, path::Path};

use openjoc_sofa::{
    BuiltinHrtf, SofaLoadLimits, load_builtin_hrir_from_asset, pack_builtin_hrir_asset,
    parse_simple_free_field_hrir,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err(
            "usage: pack-hrtf-asset <preset-id> <CDF-1 SOFA or .ojhrtf> <output.ojhrtf>".into(),
        );
    }
    let preset = BuiltinHrtf::from_id(&arguments[0]).ok_or("unknown built-in HRTF preset")?;
    let source = fs::read(&arguments[1])?;
    let loaded = if source.starts_with(b"CDF\x01") {
        parse_simple_free_field_hrir(
            &source,
            SofaLoadLimits {
                max_measurements: 70_000,
                max_file_bytes: 256 * 1024 * 1024,
                ..SofaLoadLimits::default()
            },
        )?
    } else {
        load_builtin_hrir_from_asset(preset, &source)?
    };
    let packed = pack_builtin_hrir_asset(preset, &loaded)?;
    let output = Path::new(&arguments[2]);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &packed)?;
    println!(
        "preset={} directions={} sample_rate_hz={} taps={} bytes={}",
        preset.id(),
        loaded.bank.entries().len(),
        loaded.metadata.sample_rate_hz,
        loaded.metadata.original_fir_length,
        packed.len(),
    );
    Ok(())
}
