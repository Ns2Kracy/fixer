use fixer_core::ProviderId;
use fixer_sdk::{Fixer, FixtureDocument, FixtureProvider};

fn accepts<T>(_: T) {}

fn main() {
    let provider = FixtureProvider::new(
        ProviderId::new("fixture").unwrap(),
        Vec::<FixtureDocument>::new(),
    )
    .unwrap();
    let builder = Fixer::builder().provider(provider);
    accepts(builder);
}
