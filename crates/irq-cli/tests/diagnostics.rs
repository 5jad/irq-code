use std::{path::PathBuf, process::Command};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_check(name: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check")
        .arg(fixture(name))
        .output()
        .expect("CLI executable should run")
}

#[test]
fn compile_pass_fixture_returns_success_json() {
    let output = run_check("pass/hello.irq");
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["status"], "ok");
    assert_eq!(response["module"], "hello");
    assert_eq!(response["functions"], 1);
}

#[test]
fn compile_fail_parse_fixture_uses_schema_and_stable_code() {
    let output = run_check("fail/missing_section.irq");
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema_version"], "1.0.0");
    assert_eq!(response["status"], "error");
    let diagnostic = &response["diagnostics"][0];
    assert_eq!(diagnostic["code"], "E0100");
    assert_eq!(diagnostic["kind"], "parse_error");
    assert_eq!(diagnostic["severity"], "error");
    assert_eq!(diagnostic["labels"][0]["line"], 2);
    assert!(diagnostic["labels"][0]["primary"].as_bool().unwrap());
    assert!(
        diagnostic["labels"][0]["file"]
            .as_str()
            .unwrap()
            .contains("missing_section.irq")
    );
}

#[test]
fn compile_fail_type_fixture_uses_stable_type_code() {
    let output = run_check("fail/type_mismatch.irq");
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &response["diagnostics"][0];
    assert_eq!(diagnostic["code"], "E0203");
    assert_eq!(diagnostic["kind"], "type_error");
    assert!(diagnostic["message"].as_str().unwrap().contains("expects"));
}

#[test]
fn lexical_error_has_exact_utf8_byte_span() {
    let output = run_check("fail/unexpected_character.irq");
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &response["diagnostics"][0];
    let label = &diagnostic["labels"][0];
    assert_eq!(diagnostic["code"], "E0002");
    assert_eq!(label["line"], 2);
    assert_eq!(label["column"], 5);
    assert_eq!(label["start"], 16);
    assert_eq!(label["end"], 17);
}

#[test]
fn cli_rejects_oversized_files_before_reading_them() {
    let path = std::env::temp_dir().join(format!("irq_large_fixture_{}.irq", std::process::id()));
    std::fs::write(&path, vec![b' '; irq_lexer::MAX_SOURCE_BYTES + 1]).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["diagnostics"][0]["code"], "E0001");
    assert_eq!(response["diagnostics"][0]["kind"], "resource_limit");
}

#[test]
fn checked_in_schema_matches_response_contract_version() {
    let schema: Value = serde_json::from_str(include_str!("../../../docs/diagnostic.schema.json"))
        .expect("checked-in JSON Schema must itself be valid JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], "1.0.0");
    assert_eq!(schema["$defs"]["diagnostic"]["required"][0], "code");
}
