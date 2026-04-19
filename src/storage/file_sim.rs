#[cfg(feature = "std")]
use embedded_storage::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

#[cfg(feature = "std")]
use std::fs::{File, OpenOptions};
#[cfg(feature = "std")]
use std::io::{self, Read, Seek, SeekFrom, Write};
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFlashError {
    Io(std::io::ErrorKind),
    OutOfBounds,
    NotAligned,
    RequiresErase,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for FileFlashError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind())
    }
}

#[cfg(feature = "std")]
impl NorFlashError for FileFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Self::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            Self::NotAligned => NorFlashErrorKind::NotAligned,
            Self::RequiresErase | Self::Io(_) => NorFlashErrorKind::Other,
        }
    }
}

#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct FileFlashSimulator<const WRITE_SIZE: usize, const ERASE_SIZE: usize> {
    path: PathBuf,
    capacity: usize,
}

#[cfg(feature = "std")]
impl<const WRITE_SIZE: usize, const ERASE_SIZE: usize> FileFlashSimulator<WRITE_SIZE, ERASE_SIZE> {
    pub fn new(path: impl Into<PathBuf>, capacity: usize) -> io::Result<Self> {
        let path = path.into();
        initialize_backing_file(&path, capacity)?;
        Ok(Self { path, capacity })
    }

    pub fn reopen(&self) -> io::Result<Self> {
        Self::new(self.path.clone(), self.capacity)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn capacity_bytes(&self) -> usize {
        self.capacity
    }

    fn open_rw(&self) -> Result<File, FileFlashError> {
        Ok(OpenOptions::new().read(true).write(true).open(&self.path)?)
    }
}

#[cfg(feature = "std")]
fn initialize_backing_file(path: &Path, capacity: usize) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;

    let current_len = file.metadata()?.len() as usize;
    if current_len > capacity {
        file.set_len(capacity as u64)?;
        return Ok(());
    }

    if current_len < capacity {
        file.seek(SeekFrom::Start(current_len as u64))?;
        let chunk = [0xFF_u8; 256];
        let mut remaining = capacity - current_len;
        while remaining > 0 {
            let step = remaining.min(chunk.len());
            file.write_all(&chunk[..step])?;
            remaining -= step;
        }
        file.flush()?;
    }

    Ok(())
}

#[cfg(feature = "std")]
impl<const WRITE_SIZE: usize, const ERASE_SIZE: usize> ErrorType
    for FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>
{
    type Error = FileFlashError;
}

#[cfg(feature = "std")]
impl<const WRITE_SIZE: usize, const ERASE_SIZE: usize> ReadNorFlash
    for FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>
{
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let end = offset.saturating_add(bytes.len());
        if end > self.capacity {
            return Err(FileFlashError::OutOfBounds);
        }

        let mut file = self.open_rw()?;
        file.seek(SeekFrom::Start(offset as u64))?;
        file.read_exact(bytes)?;
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(feature = "std")]
impl<const WRITE_SIZE: usize, const ERASE_SIZE: usize> NorFlash
    for FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>
{
    const WRITE_SIZE: usize = WRITE_SIZE;
    const ERASE_SIZE: usize = ERASE_SIZE;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        if !offset.is_multiple_of(WRITE_SIZE) || !bytes.len().is_multiple_of(WRITE_SIZE) {
            return Err(FileFlashError::NotAligned);
        }

        let end = offset.saturating_add(bytes.len());
        if end > self.capacity {
            return Err(FileFlashError::OutOfBounds);
        }

        let mut file = self.open_rw()?;
        file.seek(SeekFrom::Start(offset as u64))?;

        let mut existing = vec![0u8; bytes.len()];
        file.read_exact(&mut existing)?;
        for (stored, next) in existing.iter().zip(bytes.iter().copied()) {
            if (next | *stored) != *stored {
                return Err(FileFlashError::RequiresErase);
            }
        }

        for (stored, next) in existing.iter_mut().zip(bytes.iter().copied()) {
            *stored &= next;
        }

        file.seek(SeekFrom::Start(offset as u64))?;
        file.write_all(&existing)?;
        file.flush()?;
        Ok(())
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let from = from as usize;
        let to = to as usize;
        if !from.is_multiple_of(ERASE_SIZE) || !to.is_multiple_of(ERASE_SIZE) || from > to {
            return Err(FileFlashError::NotAligned);
        }
        if to > self.capacity {
            return Err(FileFlashError::OutOfBounds);
        }

        let mut file = self.open_rw()?;
        file.seek(SeekFrom::Start(from as u64))?;
        let chunk = [0xFF_u8; 256];
        let mut remaining = to - from;
        while remaining > 0 {
            let step = remaining.min(chunk.len());
            file.write_all(&chunk[..step])?;
            remaining -= step;
        }
        file.flush()?;
        Ok(())
    }
}
