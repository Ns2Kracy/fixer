use fixer_core::{
    AnimeCandidate, Candidate, ExternalId, MatchEvidenceKind, MatchQuery, Matcher, MovieCandidate,
    MusicCandidate, ProviderId,
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

fn candidate_with_sequence(id: &str, title: &str, year: Option<u16>, sequence: &str) -> Candidate {
    Candidate::Movie(
        MovieCandidate::new(
            ProviderId::new("fixture").unwrap(),
            ExternalId::new("tmdb", id).unwrap(),
            title,
            year,
        )
        .unwrap()
        .with_sequence(sequence)
        .unwrap(),
    )
}

#[test]
fn confidence_normalizes_available_evidence() {
    enum Expectation {
        Equal(f32),
        AtLeast(f32),
        Below(f32),
    }

    struct Case {
        name: &'static str,
        query: MatchQuery,
        candidate: Candidate,
        expectation: Expectation,
    }

    let cases = [
        Case {
            name: "exact external ID",
            query: MatchQuery::movie("Unrelated Query")
                .unwrap()
                .with_external_id(ExternalId::new("tmdb", "1").unwrap()),
            candidate: candidate("1", "Different Candidate", None),
            expectation: Expectation::Equal(1.0),
        },
        Case {
            name: "exact title and year",
            query: MatchQuery::movie("The Great Movie")
                .unwrap()
                .with_year(2000),
            candidate: candidate("2", "The Great Movie", Some(2000)),
            expectation: Expectation::AtLeast(0.9),
        },
        Case {
            name: "partial title",
            query: MatchQuery::movie("The Great Movie").unwrap(),
            candidate: candidate("3", "Great Movie", None),
            expectation: Expectation::Below(0.9),
        },
        Case {
            name: "negative year",
            query: MatchQuery::movie("The Great Movie")
                .unwrap()
                .with_year(2000),
            candidate: candidate("4", "The Great Movie", Some(1999)),
            expectation: Expectation::Below(0.9),
        },
        Case {
            name: "negative sequence",
            query: MatchQuery::movie("The Great Movie")
                .unwrap()
                .with_sequence("part-1")
                .unwrap(),
            candidate: candidate_with_sequence("5", "The Great Movie", None, "part-2"),
            expectation: Expectation::Below(0.9),
        },
        Case {
            name: "negative evidence clamps to zero",
            query: MatchQuery::movie("No Match")
                .unwrap()
                .with_alias("Another Title")
                .unwrap()
                .with_year(2000)
                .with_sequence("part-1")
                .unwrap(),
            candidate: candidate_with_sequence("6", "Different", Some(1999), "part-2"),
            expectation: Expectation::Equal(0.0),
        },
    ];

    for case in cases {
        let confidence = Matcher
            .score(&case.query, &case.candidate)
            .unwrap()
            .confidence();
        assert!(
            (0.0..=1.0).contains(&confidence),
            "{} confidence {confidence} was not normalized",
            case.name
        );
        match case.expectation {
            Expectation::Equal(expected) => assert!(
                (confidence - expected).abs() < f32::EPSILON,
                "{} confidence {confidence} did not equal {expected}",
                case.name
            ),
            Expectation::AtLeast(minimum) => assert!(
                confidence >= minimum,
                "{} confidence {confidence} was below {minimum}",
                case.name
            ),
            Expectation::Below(maximum) => assert!(
                confidence < maximum,
                "{} confidence {confidence} was not below {maximum}",
                case.name
            ),
        }
    }
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

    let score = Matcher.score(&query, &candidate).unwrap();
    assert!(score.total > 0);
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
