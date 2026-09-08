//! Conservative episode identity used for placement, not metadata matching.

use std::path::Path;

/// Only single-episode names are safe to attach to one episode NFO.
pub fn episode_number(path: &Path) -> Option<(u32, u32)> {
    let stem = path.file_stem()?.to_str()?;
    let bytes = stem.as_bytes();
    let mut found = None;
    for start in 0..bytes.len() {
        if !bytes[start].eq_ignore_ascii_case(&b's')
            || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            continue;
        }
        let mut cursor = start + 1;
        let Some(season) = number(bytes, &mut cursor) else {
            continue;
        };
        if !bytes
            .get(cursor)
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e'))
        {
            continue;
        }
        cursor += 1;
        let episode = number(bytes, &mut cursor)?;
        if episode == 0 || found.is_some() {
            return None;
        }
        // S01E01E02 / S01E01-E02 / S01E01-02 are not single episodes.
        let suffix = &bytes[cursor..];
        if suffix
            .first()
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e'))
            || (suffix.first() == Some(&b'-')
                && suffix
                    .get(1)
                    .is_some_and(|byte| byte.is_ascii_digit() || byte.eq_ignore_ascii_case(&b'e')))
        {
            return None;
        }
        found = Some((season, episode));
    }
    if found.is_some() {
        return found;
    }
    let folder = path.parent()?.file_name()?.to_str()?.to_ascii_lowercase();
    let season = folder.strip_prefix("season ")?.trim().parse().ok()?;
    let mut cursor = usize::from(
        bytes
            .first()
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e')),
    );
    let episode = number(bytes, &mut cursor)?;
    if episode == 0 || bytes.get(cursor).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    Some((season, episode))
}

fn number(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let start = *cursor;
    let mut number = 0_u32;
    while let Some(byte) = bytes.get(*cursor).filter(|byte| byte.is_ascii_digit()) {
        number = number
            .checked_mul(10)?
            .checked_add(u32::from(*byte - b'0'))?;
        *cursor += 1;
    }
    (*cursor > start).then_some(number)
}
