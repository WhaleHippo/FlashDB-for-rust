pub mod file_sim;
pub mod mock;
pub mod nor_flash;
pub mod region;

#[cfg(feature = "std")]
pub use file_sim::{FileFlashError, FileFlashSimulator};
pub use mock::MockFlash;
pub use nor_flash::NorFlashRegion;
pub use region::StorageRegion;
