#[cfg(feature = "std")]
#[derive(Debug)]
pub struct FileFlashSimulator {
    path: std::path::PathBuf,
}

#[cfg(feature = "std")]
impl FileFlashSimulator {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
