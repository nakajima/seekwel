#[test]
fn belongs_to_ui_errors_are_reported() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
