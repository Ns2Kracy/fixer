use fixer_core::{
    Confidence, ExternalId, LanguageTag, LocalePolicy, LocalizedValue, ProvenanceMap, ProviderId,
    SourceRef,
};
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn confidence_accepts_only_finite_unit_interval_values() {
    assert!(Confidence::new(0.0).unwrap().get().abs() < f32::EPSILON);
    assert!((Confidence::new(1.0).unwrap().get() - 1.0).abs() < f32::EPSILON);
    assert!(Confidence::new(-0.01).is_err());
    assert!(Confidence::new(1.01).is_err());
    assert!(Confidence::new(f32::NAN).is_err());
    assert!(serde_json::from_str::<Confidence>("2.0").is_err());
}

#[test]
fn language_tags_are_validated_and_preserve_the_input() {
    let tag: LanguageTag = "zh-Hant-TW".parse().unwrap();

    assert_eq!(tag.as_str(), "zh-Hant-TW");
    assert_eq!(tag.normalized(), "zh-hant-tw");
    assert_eq!(tag.primary_language(), "zh");
    assert!("not_a_tag".parse::<LanguageTag>().is_err());
    assert!(serde_json::from_str::<LanguageTag>("\"also_not_a_tag\"").is_err());
}

#[test]
fn locale_policy_selects_exact_parent_and_und_fallbacks() {
    let mut values = LocalizedValue::new();
    values.insert("zh-Hant", "繁體".to_owned()).unwrap();
    values.insert("en", "English".to_owned()).unwrap();
    values.insert("und", "Undefined".to_owned()).unwrap();
    values.insert_untagged("Untagged".to_owned());

    let exact = LocalePolicy::new(["en", "zh-Hant"]);
    assert_eq!(
        values.select(&exact.unwrap()).map(String::as_str),
        Some("English")
    );

    let parent = LocalePolicy::new(["zh-Hant-TW"]).unwrap();
    assert_eq!(values.select(&parent).map(String::as_str), Some("繁體"));

    let und = LocalePolicy::new(["ja"]).unwrap();
    assert_eq!(values.select(&und).map(String::as_str), Some("Undefined"));
}

#[test]
fn localized_values_round_trip_without_discarding_alternates() {
    let mut values = LocalizedValue::new();
    values.insert("zh-CN", "花样年华".to_owned()).unwrap();
    values
        .insert("en", "In the Mood for Love".to_owned())
        .unwrap();
    values.insert_untagged("Fallback".to_owned());

    let json = serde_json::to_string(&values).unwrap();
    let decoded: LocalizedValue<String> = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.entries().len(), 3);
    assert_eq!(decoded, values);
}

#[test]
fn external_ids_have_value_semantics() {
    let first = ExternalId::new("tmdb", "843").unwrap();
    let same = ExternalId::new("tmdb", "843").unwrap();
    let other_namespace = ExternalId::new("imdb", "843").unwrap();

    assert_eq!(first, same);
    assert_ne!(first, other_namespace);
    assert!(ExternalId::new("", "843").is_err());
    assert!(ExternalId::new("tmdb", "").is_err());
}

#[test]
fn provenance_supports_field_path_lookup() {
    let source = SourceRef::new(
        ProviderId::new("fixture.local").unwrap(),
        Some(ExternalId::new("tmdb", "843").unwrap()),
        Some("zh-CN".parse().unwrap()),
        UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    );
    let mut provenance = ProvenanceMap::new();
    provenance.add("movie.titles", source.clone()).unwrap();

    assert_eq!(provenance.sources_for("movie.titles"), &[source]);
    assert!(provenance.sources_for("movie.summary").is_empty());
    assert!(
        provenance
            .add("", SourceRef::local(ProviderId::new("local").unwrap()))
            .is_err()
    );
}
