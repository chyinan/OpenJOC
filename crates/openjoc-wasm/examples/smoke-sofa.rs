// pattern: Imperative Shell

//! Write the same project-owned SOFA fixture used by native bridge tests.

#[path = "../tests/support/sofa.rs"]
mod sofa;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: smoke-sofa OUTPUT.sofa")?;
    std::fs::write(path, sofa::custom_sofa_fixture())?;
    Ok(())
}
