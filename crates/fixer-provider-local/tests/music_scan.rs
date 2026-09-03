use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, HttpError, HttpRequest, HttpResponse,
    MediaKind, MetadataDocument, Provider, SearchRequest,
};
use fixer_provider_local::{LocalProvider, scan_music};
use std::{fs, path::Path};
use tempfile::tempdir;

#[derive(Debug)]
struct NoHttp;

impl HttpClient for NoHttp {
    fn execute(&self, _: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, HttpError>> {
        Box::pin(async { panic!("local music provider must not use HTTP") })
    }
}

fn write_cue(path: &Path, disc: u32, title: &str) {
    fs::write(
        path,
        format!(
            "PERFORMER \"Miles Davis\"\nTITLE \"Kind of Blue\"\nFILE \"disc-{disc}.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"{title}\"\n    INDEX 01 00:00:00\n"
        ),
    )
    .unwrap();
}

fn id3_track(title: &str, artist: &str, album: &str, year: &str, track: u8) -> Vec<u8> {
    fn push_field(output: &mut Vec<u8>, value: &str, width: usize) {
        output.extend_from_slice(value.as_bytes());
        output.resize(output.len() + width - value.len(), 0);
    }
    let mut output = vec![0x55; 32];
    output.extend_from_slice(b"TAG");
    push_field(&mut output, title, 30);
    push_field(&mut output, artist, 30);
    push_field(&mut output, album, 30);
    push_field(&mut output, year, 4);
    push_field(&mut output, "", 28);
    output.push(0);
    output.push(track);
    output.push(0);
    output
}

#[test]
fn cue_sheets_aggregate_into_artist_release_group_release_discs_and_tracks() {
    let root = tempdir().unwrap();
    let album = root.path().join("Miles Davis/Kind of Blue");
    fs::create_dir_all(album.join("Disc 1")).unwrap();
    fs::create_dir_all(album.join("Disc 2")).unwrap();
    write_cue(&album.join("Disc 1/album.cue"), 1, "So What");
    write_cue(&album.join("Disc 2/album.cue"), 2, "Alternate Take");

    let result = scan_music(root.path()).unwrap();

    assert!(result.warnings.is_empty());
    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.roots, vec![album]);
    let group = &result.documents[0];
    assert_eq!(group.artist.name, "Miles Davis");
    assert_eq!(group.titles.entries()[0].value(), "Kind of Blue");
    assert_eq!(group.releases.len(), 1);
    assert_eq!(group.releases[0].discs.len(), 2);
    assert_eq!(group.releases[0].discs[0].number, 1);
    assert_eq!(group.releases[0].discs[0].tracks[0].sequence.disc, 1);
    assert_eq!(group.releases[0].discs[0].tracks[0].sequence.track, 1);
    assert_eq!(
        group.releases[0].discs[0].tracks[0].titles.entries()[0].value(),
        "So What"
    );
    assert_eq!(group.releases[0].discs[1].number, 2);
}

#[test]
fn id3_tracks_group_by_artist_and_album_and_malformed_files_become_warnings() {
    let root = tempdir().unwrap();
    let album = root.path().join("Miles Davis/Kind of Blue");
    fs::create_dir_all(&album).unwrap();
    fs::write(
        album.join("01 So What.mp3"),
        id3_track("So What", "Miles Davis", "Kind of Blue", "1959", 1),
    )
    .unwrap();
    fs::write(
        album.join("02 Freddie.mp3"),
        id3_track(
            "Freddie Freeloader",
            "Miles Davis",
            "Kind of Blue",
            "1959",
            2,
        ),
    )
    .unwrap();
    fs::write(album.join("broken.mp3"), b"TAG truncated").unwrap();

    let result = scan_music(root.path()).unwrap();

    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.documents[0].releases[0].discs[0].tracks.len(), 2);
    assert_eq!(
        result.documents[0].releases[0].discs[0].tracks[0]
            .sequence
            .track,
        1
    );
    assert_eq!(
        result.documents[0].releases[0].discs[0].tracks[1]
            .sequence
            .track,
        2
    );
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].path.ends_with("broken.mp3"));
}

#[test]
fn matching_album_names_in_separate_roots_remain_distinct_releases() {
    let root = tempdir().unwrap();
    for edition in ["Original", "Remaster"] {
        let album = root.path().join(edition).join("Kind of Blue");
        fs::create_dir_all(&album).unwrap();
        fs::write(
            album.join("01 So What.mp3"),
            id3_track("So What", "Miles Davis", "Kind of Blue", "1959", 1),
        )
        .unwrap();
    }

    let result = scan_music(root.path()).unwrap();

    assert_eq!(result.documents.len(), 2);
    assert_eq!(result.roots.len(), 2);
}

#[test]
fn scanned_music_is_registered_as_a_network_free_provider_document() {
    let root = tempdir().unwrap();
    let album = root.path().join("Miles Davis/Kind of Blue");
    fs::create_dir_all(&album).unwrap();
    fs::write(
        album.join("01 So What.mp3"),
        id3_track("So What", "Miles Davis", "Kind of Blue", "1959", 1),
    )
    .unwrap();

    let (provider, warnings) = LocalProvider::from_scan(root.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(!provider.descriptor().requires_network());
    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::music("Kind of Blue", Some(1959)).unwrap(),
        &NoHttp,
    ))
    .unwrap();
    let Candidate::Music(candidate) = &candidates[0] else {
        panic!("expected music candidate");
    };
    assert_eq!(candidate.title, "Kind of Blue");
    let document = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Music, candidate.external_id.clone()),
        &NoHttp,
    ))
    .unwrap();
    assert!(matches!(document, MetadataDocument::Music(_)));
}

#[cfg(unix)]
#[test]
fn music_scan_does_not_follow_symlinked_album_directories() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(
        outside.path().join("track.mp3"),
        id3_track("Outside", "Artist", "Album", "2001", 1),
    )
    .unwrap();
    symlink(outside.path(), root.path().join("linked-album")).unwrap();

    let result = scan_music(root.path()).unwrap();
    assert!(result.documents.is_empty());
}
