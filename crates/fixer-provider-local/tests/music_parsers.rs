use fixer_provider_local::{parse_cue, parse_id3v1};

fn field<const N: usize>(value: &str) -> [u8; N] {
    let mut output = [0_u8; N];
    let bytes = value.as_bytes();
    output[..bytes.len()].copy_from_slice(bytes);
    output
}

#[test]
fn id3v11_reads_album_identity_and_track_without_touching_audio() {
    let mut bytes = vec![0x55; 32];
    bytes.extend_from_slice(b"TAG");
    bytes.extend_from_slice(&field::<30>("So What"));
    bytes.extend_from_slice(&field::<30>("Miles Davis"));
    bytes.extend_from_slice(&field::<30>("Kind of Blue"));
    bytes.extend_from_slice(b"1959");
    let mut comment = field::<30>("Columbia");
    comment[28] = 0;
    comment[29] = 1;
    bytes.extend_from_slice(&comment);
    bytes.push(8);
    let original = bytes.clone();

    let tags = parse_id3v1(&bytes).unwrap().unwrap();

    assert_eq!(tags.title.as_deref(), Some("So What"));
    assert_eq!(tags.artist.as_deref(), Some("Miles Davis"));
    assert_eq!(tags.album.as_deref(), Some("Kind of Blue"));
    assert_eq!(tags.year, Some(1959));
    assert_eq!(tags.track, Some(1));
    assert_eq!(tags.comment.as_deref(), Some("Columbia"));
    assert_eq!(bytes, original);
}

#[test]
fn id3v1_absence_and_truncation_are_distinguished() {
    assert!(parse_id3v1(&[0_u8; 128]).unwrap().is_none());
    let error = parse_id3v1(b"TAG too short").unwrap_err();
    assert!(error.to_string().contains("truncated"));
}

#[test]
fn cue_reads_global_and_per_track_identity_with_index_positions() {
    let sheet = parse_cue(
        r#"REM GENRE Jazz
PERFORMER "Miles Davis"
TITLE "Kind of Blue"
FILE "Kind of Blue.flac" WAVE
  TRACK 01 AUDIO
    TITLE "So What"
    PERFORMER "Miles Davis"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Freddie Freeloader"
    INDEX 00 09:20:00
    INDEX 01 09:22:00
FILE "Bonus.flac" WAVE
  TRACK 03 AUDIO
    TITLE "Alternate Take"
    INDEX 01 00:00:00
"#,
    )
    .unwrap();

    assert_eq!(sheet.performer.as_deref(), Some("Miles Davis"));
    assert_eq!(sheet.title.as_deref(), Some("Kind of Blue"));
    assert_eq!(sheet.files.len(), 2);
    assert_eq!(sheet.files[0].path, "Kind of Blue.flac");
    assert_eq!(sheet.files[0].tracks[0].number, 1);
    assert_eq!(sheet.files[0].tracks[0].title.as_deref(), Some("So What"));
    assert_eq!(sheet.files[0].tracks[0].index_frames, Some(0));
    assert_eq!(sheet.files[0].tracks[1].number, 2);
    assert_eq!(sheet.files[0].tracks[1].index_frames, Some(42_150));
    assert_eq!(sheet.files[1].tracks[0].number, 3);
}

#[test]
fn cue_rejects_tracks_without_a_file_or_positive_number() {
    assert!(parse_cue("TRACK 01 AUDIO\n").is_err());
    assert!(parse_cue("FILE album.flac WAVE\nTRACK 00 AUDIO\n").is_err());
}
