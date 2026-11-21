use irq_parser::parse;

const NORMATIVE_PROGRAM: &str = "mod demo\n\npub fn answer() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: result == 42\n  body:\n    return 42\n";

fn function(body: &str) -> String {
    format!(
        "mod conformance\n\nfn sample() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n{body}\n"
    )
}

#[test]
fn accepts_normative_minimal_program_and_supported_ebnf_forms() {
    assert!(parse(NORMATIVE_PROGRAM).is_ok());
    assert!(parse(&function("    return (1 + 2) * 3")).is_ok());
    assert!(parse(&function("    return 1 % 2")).is_ok());
    assert!(parse(&function("    return not false == true")).is_ok());
    assert!(parse(&function("    return \"IRQ مرحبا\"")).is_ok()); // نحوي صحيح؛ فحص النوع مرحلة منفصلة
}

#[test]
fn rejects_normative_invalid_examples_at_parse_time() {
    let missing_writes = NORMATIVE_PROGRAM.replace("  writes: none\n", "");
    let duplicate_reads =
        NORMATIVE_PROGRAM.replace("  writes: none\n", "  reads: none\n  writes: none\n");
    let reserved_function = NORMATIVE_PROGRAM.replace("answer", "return");
    let non_ascii_module = NORMATIVE_PROGRAM.replace("mod demo", "mod café");
    let tab_indentation = NORMATIVE_PROGRAM.replace("  reads:", "\treads:");
    let missing_else = function("    if true:\n      return 1");
    let chained_comparison = function("    return 1 < 2 < 3");
    let top_level_statement = format!("{NORMATIVE_PROGRAM}return 0\n");

    for (name, source) in [
        ("missing writes", missing_writes),
        ("duplicate reads", duplicate_reads),
        ("reserved function name", reserved_function),
        ("non-ASCII module name", non_ascii_module),
        ("tab indentation", tab_indentation),
        ("missing else", missing_else),
        ("chained comparison", chained_comparison),
        ("top-level statement", top_level_statement),
    ] {
        assert!(
            parse(&source).is_err(),
            "accepted normative rejection: {name}"
        );
    }
}

#[test]
fn accepts_unit_parameter_return_type_and_literal() {
    let source = "mod unit_test\n\nfn echo(value: ()) -> ()\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return value\n\nfn make() -> ()\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return ()\n";
    assert!(parse(source).is_ok());
}

#[test]
fn accepts_local_calls_mutation_while_and_supported_escapes() {
    let source = "mod control\n\nfn identity(value: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return value\n\nfn count() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let mut current: i32 = identity(0)\n    while current < 3:\n      current = current + 1\n    return current\n\nfn label() -> str\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return \"IRQ\\n\"\n";
    assert!(parse(source).is_ok());
}

#[test]
fn rejects_malformed_separators_and_misnested_blocks() {
    let no_space_after_parameter_comma = "mod bad\nfn pair(a: i32,b: i32) -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return a\n";
    let no_space_after_argument_comma = function("    return sum(1,2)");
    let misnested_else = "mod bad\nfn sample() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    if true:\n      return 1\n      else:\n        return 0\n";

    for (label, input) in [
        ("parameter separator", no_space_after_parameter_comma),
        ("argument separator", no_space_after_argument_comma.as_str()),
        ("misnested else", misnested_else),
    ] {
        assert!(parse(input).is_err(), "accepted malformed {label}");
    }
}
