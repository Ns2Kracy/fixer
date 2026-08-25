#[test]
fn public_movie_api_compiles() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/public_movie_api.rs");
}
