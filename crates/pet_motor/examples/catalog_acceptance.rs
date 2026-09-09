use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("reports/r13_catalog_acceptance.json"));
    let report = pet_motor::validate_catalog(0x6400_2026);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{}: {}/{} passed",
        output.display(),
        report.passed_program_count,
        report.catalog_program_count
    );
    if !report.passed() {
        return Err(format!("catalog failures: {:?}", report.failed_programs).into());
    }
    Ok(())
}
