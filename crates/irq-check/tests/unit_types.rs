use irq_parser::parse;

#[test]
fn accepts_unit_parameters_locals_and_returns() {
    let source = "mod unit_types\n\nfn echo(value: ()) -> ()\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    let copy: () = value\n    return copy\n";
    let module = parse(source).unwrap();
    assert!(irq_check::check(&module).is_ok());
}

#[test]
fn rejects_a_non_unit_value_returned_from_unit_function() {
    let source = "mod bad_unit\n\nfn bad() -> ()\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return true\n";
    let module = parse(source).unwrap();
    assert!(irq_check::check(&module).is_err());
}
