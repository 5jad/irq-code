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
fn parser_expression_error_reports_absolute_span_after_unicode() {
    let path = std::env::temp_dir().join(format!("irq_parse_span_{}.irq", std::process::id()));
    let source = "mod span\nfn f() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return \"مرحبا\" +\n";
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let label = &response["diagnostics"][0]["labels"][0];
    let expected = source.find('+').unwrap() + 1;
    assert_eq!(label["start"], expected);
    assert_eq!(label["end"], expected);
    assert_eq!(label["line"], 8);
    assert_eq!(label["column"], 21);
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
fn fmt_is_idempotent_normalizes_crlf_and_preserves_string_contents() {
    let path = std::env::temp_dir().join(format!("irq_fmt_{}.irq", std::process::id()));
    let source = "mod demo\r\nfn text() -> str\r\n  reads: none\r\n  writes: none\r\n  requires: true\r\n  ensures: true\r\n  body:\r\n    return \"keep   \"   \r\n\r\n\r\nfn number() -> i32\r\n  reads: none\r\n  writes: none\r\n  requires: true\r\n  ensures: true\r\n  body:\r\n    return 1\r\n";
    std::fs::write(&path, source).unwrap();
    let run_fmt = || {
        Command::new(env!("CARGO_BIN_EXE_irq"))
            .arg("fmt")
            .arg(&path)
            .output()
            .expect("CLI executable should run")
    };
    let first = run_fmt();
    assert!(first.status.success());
    let formatted = String::from_utf8(first.stdout).unwrap();
    assert!(!formatted.contains('\r'));
    assert!(formatted.contains("return \"keep   \"\n"));
    assert!(formatted.contains("\n\nfn number()"));
    let original_ast = irq_parser::parse(source).unwrap();
    let formatted_ast = irq_parser::parse(&formatted).unwrap();
    assert_eq!(original_ast.name, formatted_ast.name);
    assert_eq!(original_ast.functions.len(), formatted_ast.functions.len());
    for (original, formatted) in original_ast.functions.iter().zip(&formatted_ast.functions) {
        assert_eq!(original.name, formatted.name);
        assert_eq!(original.signature, formatted.signature);
        assert_eq!(original.body, formatted.body);
    }

    std::fs::write(&path, &formatted).unwrap();
    let second = run_fmt();
    let _ = std::fs::remove_file(path);
    assert!(second.status.success());
    assert_eq!(second.stdout, formatted.as_bytes());
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

fn temp_project() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("irq_cli_project_{}_{}", std::process::id(), unique));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("irq.toml"),
        "[package]\nname = \"demo\"\nsources = [\"src/main.irq\", \"src/math.irq\"]\n",
    )
    .unwrap();
    root
}

fn valid_module(name: &str, body: &str) -> String {
    format!(
        "mod {name}\n\nfn value() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return {body}\n"
    )
}

#[test]
fn check_project_checks_each_listed_module_and_reports_limits() {
    let root = temp_project();
    std::fs::write(root.join("src/main.irq"), valid_module("main", "1")).unwrap();
    std::fs::write(root.join("src/math.irq"), valid_module("math", "2")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check-project")
        .arg(&root)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["kind"], "project_check");
    assert_eq!(response["phase"], "per_module_subset");
    assert_eq!(response["modules"], 2);
    assert!(
        response["not_checked"]
            .as_array()
            .unwrap()
            .contains(&Value::from("cross_module_resolution"))
    );
}

#[test]
fn check_project_type_diagnostic_points_to_the_failing_source_file() {
    let root = temp_project();
    std::fs::write(root.join("src/main.irq"), valid_module("main", "1")).unwrap();
    std::fs::write(
        root.join("src/math.irq"),
        "mod math\n\nfn value() -> bool\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 2\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check-project")
        .arg(&root)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &response["diagnostics"][0];
    assert_eq!(diagnostic["kind"], "type_error");
    assert!(
        diagnostic["labels"][0]["file"]
            .as_str()
            .unwrap()
            .ends_with("src/math.irq")
    );
}

#[test]
fn check_project_parse_diagnostic_uses_the_failing_module_file_and_span() {
    let root = temp_project();
    std::fs::write(root.join("src/main.irq"), valid_module("main", "1")).unwrap();
    let bad = "mod math\n\nfn value() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 2 +\n";
    std::fs::write(root.join("src/math.irq"), bad).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_irq"))
        .arg("check-project")
        .arg(&root)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &response["diagnostics"][0];
    let label = &diagnostic["labels"][0];
    assert_eq!(diagnostic["kind"], "parse_error");
    assert!(label["file"].as_str().unwrap().ends_with("src/math.irq"));
    assert_eq!(label["start"], bad.find('+').unwrap() + 1);
}
