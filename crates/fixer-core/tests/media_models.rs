use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Asset, AssetId, AssetKind,
    AssetPath, BookEdition, BookWork, Cour, Credit, CreditRole, Disc, Duration, Episode,
    EpisodeSequence, Genre, Isbn10, Isbn13, LocalizedValue, Movie, MovieRelease, MusicArtist,
    MusicRelease, MusicReleaseGroup, OrderingScheme, Person, PersonId, ReleaseDate, ReleaseId,
    Season, Series, SourcePath, Track, TrackSequence, WorkId,
};

fn titles(primary: &str) -> LocalizedValue<String> {
    let mut titles = LocalizedValue::new();
    titles.insert("en", primary.to_owned()).unwrap();
    titles
}

#[test]
fn movie_round_trips_with_a_typed_release() {
    let director = Person::new(PersonId::new("wong-kar-wai").unwrap(), "Wong Kar-wai").unwrap();
    let mut movie = Movie::new(
        WorkId::new("movie-1").unwrap(),
        titles("In the Mood for Love"),
    );
    movie.genres.push(Genre::new("drama").unwrap());
    movie
        .credits
        .push(Credit::new(director, CreditRole::Director));
    movie.releases.push(MovieRelease::new(
        ReleaseId::new("movie-1-hk").unwrap(),
        ReleaseDate::ymd(2000, 9, 29).unwrap(),
    ));

    let encoded = serde_json::to_string(&movie).unwrap();
    let decoded: Movie = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, movie);
}

#[test]
fn television_round_trips_series_season_and_episode() {
    let episode = Episode::new(
        WorkId::new("episode-2").unwrap(),
        titles("The Second Episode"),
        EpisodeSequence::aired(1, 2).unwrap(),
    );
    let season = Season::new(WorkId::new("season-1").unwrap(), 1, vec![episode]).unwrap();
    let series = Series::new(
        WorkId::new("series-1").unwrap(),
        titles("Example Series"),
        OrderingScheme::Aired,
        vec![season],
    );

    let decoded: Series = serde_json::from_str(&serde_json::to_string(&series).unwrap()).unwrap();
    assert_eq!(decoded, series);
}

#[test]
fn anime_round_trips_cour_classification_and_numbering() {
    let episode = AnimeEpisode::new(
        WorkId::new("anime-ova-1").unwrap(),
        titles("OVA"),
        AnimeEpisodeClass::Ova,
        Some(1),
        Some(13),
    )
    .unwrap();
    let anime = AnimeSeries::new(
        WorkId::new("anime-1").unwrap(),
        titles("Example Anime"),
        AnimeSeriesRelation::Adaptation,
        vec![Cour::new(1, vec![episode]).unwrap()],
    );

    let decoded: AnimeSeries =
        serde_json::from_str(&serde_json::to_string(&anime).unwrap()).unwrap();
    assert_eq!(decoded, anime);
}

#[test]
fn anime_preserves_ona_as_a_distinct_episode_class() {
    let episode = AnimeEpisode::new(
        WorkId::new("anime-ona-1").unwrap(),
        titles("ONA"),
        AnimeEpisodeClass::Ona,
        Some(1),
        Some(1),
    )
    .unwrap();

    let encoded = serde_json::to_value(episode).unwrap();
    assert_eq!(encoded["class"], "ona");
}

#[test]
fn music_round_trips_release_group_release_disc_and_track() {
    let artist = MusicArtist::new(WorkId::new("artist-1").unwrap(), "Example Artist").unwrap();
    let track = Track::new(
        AssetId::new("track-1").unwrap(),
        titles("Opening"),
        TrackSequence::new(1, 1).unwrap(),
        Duration::from_seconds(210),
    );
    let disc = Disc::new(1, vec![track]).unwrap();
    let release = MusicRelease::new(ReleaseId::new("album-us").unwrap(), vec![disc]);
    let group = MusicReleaseGroup::new(
        WorkId::new("album-work").unwrap(),
        titles("Example Album"),
        artist,
        vec![release],
    );

    let decoded: MusicReleaseGroup =
        serde_json::from_str(&serde_json::to_string(&group).unwrap()).unwrap();
    assert_eq!(decoded, group);
}

#[test]
fn book_round_trips_edition_contributors_and_file_asset() {
    let contributor = Person::new(
        PersonId::new("ursula-le-guin").unwrap(),
        "Ursula K. Le Guin",
    )
    .unwrap();
    let asset = Asset::new(
        AssetId::new("ebook-1").unwrap(),
        SourcePath::new("books/earthsea.epub").unwrap(),
        AssetKind::BookFile,
    );
    let edition = BookEdition::new(
        ReleaseId::new("earthsea-edition").unwrap(),
        Isbn10::new("0547773749").unwrap(),
        Isbn13::new("9780547773742").unwrap(),
        "Houghton Mifflin Harcourt",
        vec![asset],
    )
    .unwrap();
    let book = BookWork::new(
        WorkId::new("earthsea").unwrap(),
        titles("A Wizard of Earthsea"),
        vec![Credit::new(contributor, CreditRole::Author)],
        vec![edition],
    );

    let decoded: BookWork = serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap();
    assert_eq!(decoded, book);
}

#[test]
fn asset_paths_are_facts_and_do_not_touch_the_filesystem() {
    let path = AssetPath::new("Movies/Film (2000)/Film.mkv").unwrap();
    assert_eq!(path.as_str(), "Movies/Film (2000)/Film.mkv");
    assert!(AssetPath::new("").is_err());
}
