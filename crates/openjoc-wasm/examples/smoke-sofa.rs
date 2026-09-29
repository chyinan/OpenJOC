// pattern: Imperative Shell

//! Write the same project-owned SOFA fixture used by native bridge tests.

#[path = "../tests/support/sofa.rs"]
mod sofa;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: smoke-sofa OUTPUT.sofa")?;
    std::fs::write(&path, sofa::custom_sofa_fixture())?;
    for (suffix, rate, oversized) in [
        (".hdf5.sofa", 48_000, false),
        (".24k.sofa", 24_000, false),
        (".largechunk.sofa", 48_000, true),
    ] {
        let mut output = path.clone();
        output.push(suffix);
        std::fs::write(output, sofa::custom_hdf5_sofa_fixture(rate, oversized))?;
    }
    Ok(())
}
