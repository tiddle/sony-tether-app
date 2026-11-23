use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
