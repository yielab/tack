//! `briefing()`'s output for every brief-v1 fixture validates against the
//! schema `prill schema briefing` prints (prill ADR 0014 L-3). It needs prill
//! on `PATH`; CI has none and the test says so and passes.

use std::{fs, path::PathBuf, process::Command};

use serde_json::Value;
use tack_runner::verify::briefing;

#[test]
fn every_brief_fixture_translates_into_a_valid_briefing() {
    let output = match Command::new("prill").args(["schema", "briefing"]).output() {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            println!(
                "skipped: `prill schema briefing` exited with {}; this prill has no schema command",
                output.status
            );
            return;
        }
        Err(error) => {
            println!("skipped: prill is not on PATH ({error})");
            return;
        }
    };
    let schema: Value = serde_json::from_slice(&output.stdout).expect("prill prints JSON");
    let validator = jsonschema::validator_for(&schema).expect("prill's schema compiles");

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/contracts/brief-v1");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("brief-v1 directory") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|ext| ext != "json")
            || path.file_name().is_some_and(|name| name == "schema.json")
        {
            continue;
        }
        let brief: Value =
            serde_json::from_slice(&fs::read(&path).expect("fixture")).expect("json");
        let document = briefing(&brief);
        let errors: Vec<String> = validator
            .iter_errors(&document)
            .map(|error| format!("{error} at {}", error.instance_path))
            .collect();
        assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
        checked += 1;
    }
    assert!(checked > 0, "no brief-v1 fixture found");
}
