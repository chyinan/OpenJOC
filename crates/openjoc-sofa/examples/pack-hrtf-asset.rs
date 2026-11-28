// pattern: Imperative Shell

use std::{env, fs, path::Path};

use openjoc_sofa::{
    BuiltinHrtf, LoadedSofaHrirBank, SofaError, SofaLoadLimits, load_builtin_hrir_from_asset,
    pack_builtin_hrir_asset, parse_simple_free_field_hrir,
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
    let (packed, loaded) = prepare_asset(preset, source)?;
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

fn prepare_asset(
    preset: BuiltinHrtf,
    source: Vec<u8>,
) -> Result<(Vec<u8>, LoadedSofaHrirBank), SofaError> {
    if source.starts_with(b"CDF\x01") {
        let loaded = parse_simple_free_field_hrir(
            &source,
            SofaLoadLimits {
                max_measurements: 70_000,
                max_file_bytes: 256 * 1024 * 1024,
                ..SofaLoadLimits::default()
            },
        )?;
        let packed = pack_builtin_hrir_asset(preset, &loaded)?;
        Ok((packed, loaded))
    } else {
        // Loading validates the registry checksum but may normalize a direction.
        // Preserve the verified bytes so the output remains a loadable asset.
        let loaded = load_builtin_hrir_from_asset(preset, &source)?;
        Ok((source, loaded))
    }
}

#[cfg(all(test, feature = "embedded-builtin-hrtf"))]
mod tests {
    use super::*;
    use openjoc_sofa::builtin_hrtf_asset_bytes;

    #[test]
    fn verified_packed_inputs_keep_the_registry_bytes() {
        for preset in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            let original = builtin_hrtf_asset_bytes(preset).unwrap();
            let (output, _) = prepare_asset(preset, original.to_vec()).unwrap();
            assert!(output == original, "changed bytes for {}", preset.id());
            assert!(load_builtin_hrir_from_asset(preset, &output).is_ok());
        }
    }

    #[test]
    fn packed_input_for_another_preset_is_rejected() {
        let original = builtin_hrtf_asset_bytes(BuiltinHrtf::SadieD2Kemar).unwrap();
        assert!(prepare_asset(BuiltinHrtf::SadieD1Ku100, original.to_vec()).is_err());
    }
}
