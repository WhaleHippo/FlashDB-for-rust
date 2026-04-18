use crate::config::KvConfig;

#[derive(Debug)]
pub struct KvDb {
    pub config: KvConfig,
}

impl KvDb {
    pub const fn new(config: KvConfig) -> Self {
        Self { config }
    }
}
