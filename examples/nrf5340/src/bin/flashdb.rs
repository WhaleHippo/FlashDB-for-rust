#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_alloc::LlffHeap;
use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::MockFlash;
use flashdb_for_rust::storage::mock::MockFlashError;
use flashdb_for_rust::tsdb::TsDb;
use flashdb_for_rust::{
    BlobMode, Error as FlashError, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig,
};
use {defmt_rtt as _, panic_probe as _};

#[global_allocator]
static ALLOCATOR: LlffHeap = LlffHeap::empty();

const HEAP_SIZE: usize = 32 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

type ExampleFlash = MockFlash<4096, 4, 1024>;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    init_heap();
    let _p = embassy_nrf::init(Default::default());

    info!("nRF5340 FlashDB embedded example start");
    run_flashdb_smoke().unwrap_or_else(|_| panic!("FlashDB smoke failed on nRF5340"));
    info!("nRF5340 FlashDB smoke passed");

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

fn init_heap() {
    unsafe {
        ALLOCATOR.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
}

fn run_flashdb_smoke() -> Result<(), FlashError<MockFlashError>> {
    let mut kv = KvDb::mount(ExampleFlash::new(), kv_config())?;
    kv.format()?;
    kv.set("board", b"nrf5340")?;

    let mut kv_buf = [0u8; 16];
    let Some(kv_len) = kv.get_blob_into("board", &mut kv_buf)? else {
        panic!("missing KV value after write");
    };
    if &kv_buf[..kv_len] != b"nrf5340" {
        panic!("unexpected KV payload");
    }

    let mut ts = TsDb::mount(ExampleFlash::new(), ts_config())?;
    ts.format()?;
    ts.append(1, b"cold")?;
    ts.append(2, b"warm")?;

    let mut reverse = ts.iter_reverse()?;
    let Some(latest) = reverse.next() else {
        panic!("missing TS record");
    };
    if latest.timestamp != 2 || latest.payload.as_slice() != b"warm" {
        panic!("unexpected TS payload");
    }

    Ok(())
}

fn kv_config() -> KvConfig {
    KvConfig {
        region: StorageRegionConfig::new(0, 2048, 1024, 4),
        max_key_len: 32,
        max_value_len: 64,
    }
}

fn ts_config() -> TsdbConfig {
    TsdbConfig {
        region: StorageRegionConfig::new(0, 2048, 1024, 4),
        blob_mode: BlobMode::Variable,
        timestamp_policy: TimestampPolicy::StrictMonotonic,
        rollover: false,
    }
}
