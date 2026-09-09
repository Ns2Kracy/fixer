use fixer_core::{
    AnimeCandidate, Candidate, ExternalId, MatchQuery, Matcher, MovieCandidate, MusicCandidate,
    ProviderId, RankedCandidate,
};

fn candidate(provider: &str, id: &str, title: &str, year: Option<u16>) -> Candidate {
    Candidate::Movie(
        MovieCandidate::new(
            ProviderId::new(provider).unwrap(),
            ExternalId::new("tmdb", id).unwrap(),
            title,
            year,
        )
        .unwrap(),
    )
}

fn ranked_ids(ranked: &[RankedCandidate]) -> Vec<&str> {
    ranked
        .iter()
        .map(|ranked| ranked.candidate.external_id().value.as_str())
        .collect()
}

#[test]
fn candidates_use_categorical_deterministic_order() {
    let query = MatchQuery::movie("The Great Movie")
        .unwrap()
        .with_year(2000)
        .with_external_id(ExternalId::new("tmdb", "external-id").unwrap());

    let ranked = Matcher
        .rank(
            &query,
            vec![
                candidate("first", "native", "Different", Some(2000)),
                candidate("first", "title", "  THE great   movie ", Some(1999)),
                candidate("first", "title-year", "The Great Movie", Some(2000)),
                candidate("second", "external-id", "Different", Some(1999)),
            ],
        )
        .unwrap();

    assert_eq!(
        ranked_ids(&ranked),
        ["external-id", "title-year", "title", "native"]
    );
}

#[test]
fn equal_matches_keep_provider_and_native_order() {
    let query = MatchQuery::movie("No Match").unwrap();
    let input = vec![
        candidate("z-provider", "z-first", "Different", None),
        candidate("z-provider", "z-second", "Also Different", None),
        candidate("a-provider", "a-first", "Still Different", None),
        candidate("a-provider", "a-second", "Entirely Different", None),
    ];

    let selection = Matcher.select(&query, input).unwrap();

    assert_eq!(
        ranked_ids(selection.ranked()),
        ["z-first", "z-second", "a-first", "a-second"]
    );
    assert_eq!(
        selection.selected().unwrap().candidate.external_id().value,
        "z-first"
    );
}

#[test]
fn exact_external_ids_outrank_exact_titles() {
    let query = MatchQuery::movie("Completely Different")
        .unwrap()
        .with_external_id(ExternalId::new("tmdb", "843").unwrap());
    let exact_id = candidate("fixture", "843", "Unrelated", Some(1990));
    let exact_title = candidate("fixture", "999", "Completely Different", Some(2000));

    let ranked = Matcher.rank(&query, vec![exact_title, exact_id]).unwrap();

    assert_eq!(ranked[0].candidate.external_id().value, "843");
}

#[test]
fn localized_titles_and_aliases_are_exact_title_matches() {
    let mut query = MatchQuery::movie("In the Mood for Love")
        .unwrap()
        .with_alias("Fa yeung nin wa")
        .unwrap();
    query.add_localized_title("zh-CN", "花样年华").unwrap();

    let ranked = Matcher
        .rank(
            &query,
            vec![
                candidate("fixture", "native", "Different", None),
                candidate("fixture", "localized", "花样年华", None),
                candidate("fixture", "alias", "Fa Yeung Nin Wa", None),
            ],
        )
        .unwrap();

    assert_eq!(ranked_ids(&ranked), ["localized", "alias", "native"]);
}

#[test]
fn ranked_candidate_has_no_score_or_confidence_contract() {
    let ranked = Matcher
        .rank(
            &MatchQuery::movie("Movie").unwrap(),
            vec![candidate("fixture", "1", "Movie", None)],
        )
        .unwrap();

    let json = serde_json::to_value(&ranked[0]).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 1);
    assert!(json.get("candidate").is_some());
    assert!(json.get("score").is_none());
    assert!(json.get("confidence").is_none());
}

#[test]
fn music_queries_rank_music_candidates() {
    let query = MatchQuery::music("Kind of Blue").unwrap().with_year(1959);
    let candidate = Candidate::Music(
        MusicCandidate::new(
            ProviderId::new("musicbrainz").unwrap(),
            ExternalId::new("musicbrainz-release-group", "1f2a").unwrap(),
            "Kind of Blue",
            Some(1959),
        )
        .unwrap(),
    );

    let selection = Matcher.select(&query, vec![candidate]).unwrap();
    assert!(selection.selected().is_some());
}

#[test]
fn anime_queries_rank_typed_anime_candidates() {
    let query = MatchQuery::anime("葬送のフリーレン").unwrap();
    let candidate = Candidate::Anime(
        AnimeCandidate::new(
            ProviderId::new("bangumi").unwrap(),
            ExternalId::new("bangumi", "400602").unwrap(),
            "葬送のフリーレン",
            Some(2023),
        )
        .unwrap(),
    );

    let selection = Matcher.select(&query, vec![candidate]).unwrap();
    assert!(selection.selected().is_some());
}

#[test]
fn selecting_no_candidates_returns_an_error() {
    let error = Matcher
        .select(&MatchQuery::movie("Movie").unwrap(), vec![])
        .unwrap_err();

    assert_eq!(error, fixer_core::MatchingError::NoCandidates);
}
