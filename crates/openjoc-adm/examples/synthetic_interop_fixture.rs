use openjoc_adm::{AdmExportPlan, AdmPolicy, StreamingAdmWriter};
use openjoc_scene::SemanticBindingState;
use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: synthetic_interop_fixture OUTPUT.wav")?;
    let duration_samples = 48_000_u64;
    let plan = AdmExportPlan::new(
        48_000,
        duration_samples,
        2,
        true,
        0,
        0,
        SemanticBindingState::Unresolved,
        AdmPolicy::BestEffort,
    )?;
    let file = fs::File::create(&output)?;
    let mut writer = StreamingAdmWriter::new(file, plan)?;
    let mut remaining = duration_samples;
    while remaining > 0 {
        let frames = usize::try_from(remaining.min(1_536))?;
        writer.write_pcm(
            &[vec![0.0; frames], vec![0.0; frames]],
            Some(&vec![0.0; frames]),
        )?;
        remaining -= u64::try_from(frames)?;
    }
    let (file, mut report, _) = writer.finish()?;
    file.sync_all()?;
    report.source_format = "project-owned synthetic silence";
    report.source_is_lossy_e_ac_3_joc = false;
    fs::write(
        output.with_extension("adm-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
