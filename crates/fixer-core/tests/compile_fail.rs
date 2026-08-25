#[test]
fn media_domain_types_do_not_interchange() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
