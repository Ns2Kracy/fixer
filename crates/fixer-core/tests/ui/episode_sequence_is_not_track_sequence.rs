use fixer_core::{AssetId, Duration, EpisodeSequence, LocalizedValue, Track};

fn main() {
    let titles = LocalizedValue::<String>::new();
    let sequence = EpisodeSequence::aired(1, 2).unwrap();
    let _track = Track::new(
        AssetId::new("track-1").unwrap(),
        titles,
        sequence,
        Duration::from_seconds(60),
    );
}
