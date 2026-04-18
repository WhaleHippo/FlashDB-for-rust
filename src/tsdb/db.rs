use crate::config::TsdbConfig;

#[derive(Debug)]
pub struct TsDb {
    pub config: TsdbConfig,
}

impl TsDb {
    pub const fn new(config: TsdbConfig) -> Self {
        Self { config }
    }
}
