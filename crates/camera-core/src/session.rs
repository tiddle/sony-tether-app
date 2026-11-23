use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{Result, TetherError};

/// One shoot on disk: a folder plus the rule for naming files inside it.
///
/// Kept free of camera knowledge so it can be tested without hardware.
#[derive(Clone, Debug)]
pub struct Session {
    root: PathBuf,
    prefix: String,
    counter: u32,
}

impl Session {
    pub fn new(root: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            prefix: prefix.into(),
            counter: 1,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the folder if it is missing. Safe to call more than once.
    pub fn ensure_root(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        Ok(())
    }

    /// Next filename in the sequence, for example `capture_0001.ARW`.
    ///
    /// The caller supplies the extension because only the backend knows the
    /// format it received.
    pub fn next_filename(&mut self, extension: &str) -> Result<String> {
        validate_extension(extension)?;
        let name = format!("{}_{:04}.{}", self.prefix, self.counter, extension);
        self.counter += 1;
        Ok(name)
    }

    /// Full path for the next capture. Does not advance the counter.
    pub fn peek_path(&self, extension: &str) -> Result<PathBuf> {
        validate_extension(extension)?;
        Ok(self
            .root
            .join(format!("{}_{:04}.{}", self.prefix, self.counter, extension)))
    }

    /// Advance the counter past a number already used, so a restart in the same
    /// folder does not overwrite yesterday's shots.
    pub fn reserve_through(&mut self, used: u32) {
        self.counter = self.counter.max(used + 1);
    }
}

fn validate_extension(extension: &str) -> Result<()> {
    let ok = !extension.is_empty()
        && extension.len() <= 8
        && extension
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.');
    if ok {
        Ok(())
    } else {
        Err(TetherError::Config(format!(
            "invalid file extension: {extension:?}"
        )))
    }
}

/// Extensions the review view can decode and show directly.
pub const VIEWABLE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

/// Extensions that are camera RAW. Listed apart because showing them needs the
/// JPEG preview embedded inside the file, which we do not extract yet.
pub const RAW_EXTENSIONS: &[&str] = &["arw", "tif", "tiff"];

/// Lowercase extension of `path`, if it has one.
pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}

pub fn is_viewable_extension(extension: &str) -> bool {
    VIEWABLE_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
}

pub fn is_raw_extension(extension: &str) -> bool {
    RAW_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
}

/// Newest file in `dir` that the review view can show.
///
/// A missing or unreadable directory means "nothing to show", not an error: on a
/// first run there is no folder yet, and that is normal.
pub fn newest_viewable(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<(SystemTime, PathBuf)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match extension_of(&path) {
            Some(extension) if is_viewable_extension(&extension) => {}
            _ => continue,
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(best, _)| modified > *best) {
            newest = Some((modified, path));
        }
    }

    newest.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    #[test]
    fn names_are_zero_padded_and_sequential() {
        let mut session = Session::new("/tmp/shoot", "capture");
        assert_eq!(session.next_filename("ARW").unwrap(), "capture_0001.ARW");
        assert_eq!(session.next_filename("ARW").unwrap(), "capture_0002.ARW");
        assert_eq!(session.next_filename("JPG").unwrap(), "capture_0003.JPG");
    }

    #[test]
    fn peek_does_not_advance() {
        let session = Session::new("/tmp/shoot", "capture");
        assert_eq!(
            session.peek_path("ARW").unwrap(),
            PathBuf::from("/tmp/shoot/capture_0001.ARW")
        );
        assert_eq!(
            session.peek_path("ARW").unwrap(),
            session.peek_path("ARW").unwrap()
        );
    }

    #[test]
    fn reserve_through_skips_used_numbers() {
        let mut session = Session::new("/tmp/shoot", "capture");
        session.reserve_through(41);
        assert_eq!(session.next_filename("ARW").unwrap(), "capture_0042.ARW");
    }

    #[test]
    fn path_separators_are_rejected() {
        let mut session = Session::new("/tmp/shoot", "capture");
        assert!(session.next_filename("../x").is_err());
        assert!(session.next_filename("a/b").is_err());
        assert!(session.next_filename("").is_err());
    }

    fn write_at(dir: &Path, name: &str, offset_secs: u64) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"x").unwrap();
        let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + offset_secs);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
        path
    }

    #[test]
    fn newest_viewable_picks_the_most_recent_image() {
        let tmp = tempfile::tempdir().unwrap();
        write_at(tmp.path(), "capture_0001.jpg", 10);
        let newer = write_at(tmp.path(), "capture_0002.png", 20);
        write_at(tmp.path(), "notes.txt", 30);

        assert_eq!(
            newest_viewable(tmp.path()).as_deref(),
            Some(newer.as_path())
        );
    }

    #[test]
    fn newest_viewable_ignores_raw_for_now() {
        let tmp = tempfile::tempdir().unwrap();
        let jpeg = write_at(tmp.path(), "capture_0001.jpg", 10);
        write_at(tmp.path(), "capture_0002.arw", 20);

        assert_eq!(newest_viewable(tmp.path()).as_deref(), Some(jpeg.as_path()));
    }

    #[test]
    fn newest_viewable_tolerates_an_absent_folder() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(newest_viewable(&tmp.path().join("missing")), None);
        assert_eq!(newest_viewable(tmp.path()), None);
    }

    #[test]
    fn extension_classification() {
        assert!(is_viewable_extension("jpg"));
        assert!(is_viewable_extension("JPEG"));
        assert!(is_raw_extension("arw"));
        assert!(!is_viewable_extension("arw"));
        assert!(!is_raw_extension("jpg"));
        assert_eq!(
            extension_of(Path::new("/a/photo.ARW")).as_deref(),
            Some("arw")
        );
        assert_eq!(extension_of(Path::new("/a/photo")), None);
    }
}
