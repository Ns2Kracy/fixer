use fixer_core::{
    AnimeCandidate, Candidate, ExternalId, MatchEvidenceKind, MatchQuery, Matcher, MovieCandidate,
    ProviderId,
};

fn candidate(id: &str, title: &str, year: Option<u16>) -> Candidate {
    Candidate::Movie(
        MovieCandidate::new(
            ProviderId::new("fixture").unwrap(),
            ExternalId::new("tmdb", id).unwrap(),
            title,
            year,
        )
        .unwrap(),
    )
}

#[test]
fn exact_external_ids_outrank_fuzzy_title_evidence() {
    let query = MatchQuery::movie("Completely Different")
        .unwrap()
        .with_external_id(ExternalId::new("tmdb", "843").unwrap());
    let exact_id = candidate("843", "Unrelated", Some(1990));
    let exact_title = candidate("999", "Completely Different", Some(2000));

    let ranked = Matcher.rank(&query, vec![exact_title, exact_id]).unwrap();
    assert_eq!(ranked[0].candidate.external_id().value, "843");
    assert!(
        ranked[0]
            .score
            .evidence
            .iter()
            .any(|item| item.kind == MatchEvidenceKind::ExternalId && item.points > 0)
    );
}

#[test]
fn matcher_exposes_positive_and_negative_evidence() {
    struct Case {
        query: MatchQuery,
        candidate: Candidate,
        expected_kind: MatchEvidenceKind,
        positive: bool,
    }

    let mut localized = MatchQuery::movie("花样年华").unwrap();
    localized.add_localized_title("zh-CN", "花样年华").unwrap();
    let cases = [
        Case {
            query: localized,
            candidate: candidate("1", "花样年华", Some(2000)),
            expected_kind: MatchEvidenceKind::Title,
            positive: true,
        },
        Case {
            query: MatchQuery::movie("In the Mood for Love")
                .unwrap()
                .with_alias("Fa yeung nin wa")
                .unwrap(),
            candidate: candidate("2", "Fa Yeung Nin Wa", Some(2000)),
            expected_kind: MatchEvidenceKind::Alias,
            positive: true,
        },
        Case {
            query: MatchQuery::movie("Movie").unwrap().with_year(2000),
            candidate: candidate("3", "Movie", Some(1990)),
            expected_kind: MatchEvidenceKind::Year,
            positive: false,
        },
        Case {
            query: MatchQuery::movie("Movie")
                .unwrap()
                .with_alias("Alternate Title")
                .unwrap(),
            candidate: candidate("4", "Different", Some(2000)),
            expected_kind: MatchEvidenceKind::Alias,
            positive: false,
        },
    ];

    for case in cases {
        let score = Matcher.score(&case.query, &case.candidate).unwrap();
        assert!(
            score.evidence.iter().any(|item| {
                item.kind == case.expected_kind && (item.points > 0) == case.positive
            })
        );
    }
}

#[test]
fn anime_queries_score_typed_anime_candidates() {
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

    let score = Matcher.score(&query, &candidate).unwrap();
    assert!(score.total > 0);
}

#[test]
fn equal_top_scores_are_reported_as_ambiguous() {
    let query = MatchQuery::movie("Movie").unwrap().with_year(2000);
    let outcome = Matcher
        .select(
            &query,
            vec![
                candidate("1", "Movie", Some(2000)),
                candidate("2", "Movie", Some(2000)),
            ],
        )
        .unwrap();

    assert!(outcome.is_ambiguous());
    assert_eq!(outcome.ranked().len(), 2);
}
