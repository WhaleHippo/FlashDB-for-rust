# FlashDB-for-rust 사용 및 포팅 가이드

이 문서는 현재 저장소의 example crate들을 바탕으로 FlashDB-for-rust를 실제 프로젝트에서 어떻게 시작하면 되는지 정리한다.

기준 example:
- Linux host smoke example: `examples/linux/src/main.rs`
- STM32F401RE Embassy example: `examples/stm32f401re/src/main.rs`
- nRF5340 Embassy example: `examples/nrf5340/src/main.rs`

이 문서는 두 가지를 목표로 한다.
1. **How to use**: Linux host에서 가장 빠르게 KVDB/TSDB를 써보는 방법
2. **How to port**: embedded target에서 example을 자기 보드/flash backend로 옮기는 방법

---

## 1. 가장 빠른 시작: Linux host에서 사용해보기

현재 가장 쉬운 시작점은 Linux example이다.
이 예제는 `std` feature + file-backed simulator를 사용해서 실제 파일에 데이터를 저장하고, 재마운트(reopen)까지 확인한다.

### 1.1 실행

저장소 루트에서:

```bash
cargo run --manifest-path examples/linux/Cargo.toml
```

성공하면 대략 아래 의미의 로그가 나온다.
- Linux host example 시작
- KV write/read/reboot round-trip 성공
- TS append/query/reboot round-trip 성공
- example 통과

### 1.2 이 예제가 실제로 보여주는 것

`examples/linux/src/main.rs`는 아래 흐름을 한 번에 보여준다.

#### KVDB
1. `KvConfig` 생성
2. `FileFlashSimulator`로 file-backed flash 생성
3. `KvDb::mount(...)`
4. `format()`
5. `set("platform", b"linux")`
6. `get_blob_into(...)`로 읽기
7. `into_flash().reopen()`으로 재부팅 유사 상황 재현
8. 재마운트 후 데이터가 남아 있는지 확인

#### TSDB
1. `TsdbConfig` 생성
2. `TsDb::mount(...)`
3. `format()`
4. `append(timestamp, payload)`
5. `iter()`, `iter_reverse()` 확인
6. `into_flash().reopen()` 후 재마운트
7. `iter_by_time(...)`, `query_count(...)` 확인

즉, **새 사용자에게는 Linux example이 가장 좋은 API 입문서**다.

---

## 2. 사용 API의 최소 패턴

examples를 기준으로 보면 보통 아래 순서로 사용하면 된다.

### 2.1 공통 설정 타입

루트 crate에서 주로 쓰는 타입:

```rust
use flashdb_for_rust::{
    BlobMode, KvConfig, StorageRegionConfig, TimestampPolicy, TsdbConfig,
};
```

### 2.2 KVDB 시작 패턴

```rust
use flashdb_for_rust::kv::KvDb;

let config = KvConfig {
    region: StorageRegionConfig::new(0, 2048, 1024, 4),
    max_key_len: 32,
    max_value_len: 64,
};

let mut kv = KvDb::mount(flash, config)?;
kv.format()?;
kv.set("board", b"stm32f401re")?;

let mut buf = [0u8; 32];
if let Some(len) = kv.get_blob_into("board", &mut buf)? {
    let value = &buf[..len];
    // value 사용
}
```

### 2.3 TSDB 시작 패턴

```rust
use flashdb_for_rust::tsdb::TsDb;

let config = TsdbConfig {
    region: StorageRegionConfig::new(0, 2048, 1024, 4),
    blob_mode: BlobMode::Variable,
    timestamp_policy: TimestampPolicy::StrictMonotonic,
    rollover: false,
};

let mut ts = TsDb::mount(flash, config)?;
ts.format()?;
ts.append(1, b"cold")?;
ts.append(2, b"warm")?;

let forward = ts.iter()?;
let reverse = ts.iter_reverse()?;
let window = ts.iter_by_time(2, 1)?;
let count = ts.query_count(1, 2, flashdb_for_rust::layout::ts::TSL_WRITE)?;
```

### 2.4 재부팅/재마운트 검증 패턴

Linux example처럼 backend를 다시 열 수 있으면:

```rust
let flash = kv.into_flash();
let reopened = flash.reopen()?;
let mut kv = KvDb::mount(reopened, config)?;
```

TSDB도 동일한 패턴을 쓴다.

---

## 3. Linux host에서 프로젝트에 붙이는 방법

Linux host에서 가장 쉬운 통합 방법은 example과 동일하게 `std` feature를 켜고 `FileFlashSimulator`를 사용하는 것이다.

### 3.1 의존성 예시

```toml
[dependencies]
flashdb-for-rust = { path = "../flashdb-for-rust", features = ["std"] }
```

### 3.2 simulator 사용 예시

```rust
use flashdb_for_rust::kv::KvDb;
use flashdb_for_rust::storage::FileFlashSimulator;
use flashdb_for_rust::{KvConfig, StorageRegionConfig};

const FLASH_BYTES: usize = 4096;
const WRITE_SIZE: usize = 4;
const ERASE_SIZE: usize = 1024;

type HostFlash = FileFlashSimulator<WRITE_SIZE, ERASE_SIZE>;

let config = KvConfig {
    region: StorageRegionConfig::new(0, 2048, ERASE_SIZE as u32, WRITE_SIZE as u32),
    max_key_len: 32,
    max_value_len: 64,
};

let flash = HostFlash::new("/tmp/flashdb.bin", FLASH_BYTES)?;
let mut kv = KvDb::mount(flash, config)?;
```

### 3.3 언제 Linux simulator가 유용한가

- API를 먼저 익히고 싶을 때
- reboot/recovery 동작을 파일 기반으로 확인하고 싶을 때
- embedded flash driver를 붙이기 전에 데이터 모델/검증 흐름부터 만들고 싶을 때

---

## 4. embedded example은 무엇을 보여주나

`examples/stm32f401re`와 `examples/nrf5340`는 둘 다 Embassy 기반 `no_std` example이다.

다만 중요한 점:
- 현재 example은 **실제 내장 flash driver를 직접 쓰는 데모가 아니다**.
- 둘 다 `MockFlash<4096, 4, 1024>`를 사용한다.
- 목적은 **FlashDB core가 allocator 없이 embedded target에서 빌드되고 smoke flow가 동작함을 증명하는 것**이다.

즉, embedded example은 아래 두 가지를 보여준다.
1. Embassy 앱에서 FlashDB를 어떤 형태로 감싸는지
2. 나중에 실제 flash backend로 교체할 때 코드 골격을 어떻게 잡는지

---

## 5. 보드로 포팅하는 기본 절차

자기 보드로 옮길 때는 아래 순서로 생각하면 된다.

### Step 1. example 하나를 출발점으로 복사

- STM32 계열이면 `examples/stm32f401re/src/main.rs`
- nRF 계열이면 `examples/nrf5340/src/main.rs`

처음에는 `run_flashdb_smoke()` 구조를 그대로 유지하는 것이 좋다.

### Step 2. board init만 자기 보드에 맞게 교체

예:
- `embassy_stm32::init(Default::default())`
- `embassy_nrf::init(Default::default())`

이 부분을 자신의 HAL/보드 초기화 코드로 바꾼다.

### Step 3. `MockFlash`를 실제 flash backend로 교체

현재 example의 핵심 alias는 이런 식이다.

```rust
type ExampleFlash = MockFlash<4096, 4, 1024>;
```

실제 포팅에서는 이 자리를 **자기 flash driver 타입**으로 바꿔야 한다.

조건:
- `embedded_storage::nor_flash::NorFlash`를 만족해야 함
- `WRITE_SIZE`와 `ERASE_SIZE`가 실제 하드웨어와 맞아야 함
- 읽기/쓰기/erase가 NOR flash semantics를 지켜야 함

즉, FlashDB-for-rust는 보통 아래 형태의 backend를 기대한다.

```rust
use embedded_storage::nor_flash::NorFlash;

struct MyFlash {
    // HAL driver, peripheral, lock 등
}

impl embedded_storage::nor_flash::ErrorType for MyFlash {
    type Error = MyFlashError;
}

impl embedded_storage::nor_flash::ReadNorFlash for MyFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        // board-specific read
        # todo!()
    }

    fn capacity(&self) -> usize {
        # todo!()
    }
}

impl embedded_storage::nor_flash::NorFlash for MyFlash {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = 1024;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        // board-specific program
        # todo!()
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        // board-specific erase
        # todo!()
    }
}
```

### Step 4. region 설정을 실제 flash map에 맞게 조정

examples의 설정은 smoke용이다.

```rust
StorageRegionConfig::new(0, 2048, 1024, 4)
```

의미:
- offset: `0`
- length: `2048`
- erase size: `1024`
- write size: `4`

실제 포팅 시에는 반드시 아래를 다시 잡아야 한다.
- FlashDB가 사용할 시작 offset
- 전체 길이
- sector erase 단위
- write/program 단위

예를 들어 앱 전체 flash 중 일부만 DB로 쓸 거라면, 그 영역의 offset/len을 명확히 잡아야 한다.

### Step 5. KV/TSDB smoke를 먼저 살린다

포팅 초반에는 기능을 많이 넣기보다 example처럼 최소 흐름만 먼저 통과시키는 게 좋다.

권장 순서:
1. `KvDb::mount()` 성공
2. `kv.format()` 성공
3. `kv.set()` / `kv.get_blob_into()` 성공
4. `TsDb::mount()` 성공
5. `ts.format()` 성공
6. `ts.append()` / `ts.iter_reverse()` 성공

이 단계가 되면 그 다음에 reboot/recovery, 실제 persistence, status mutation 등을 늘리는 게 좋다.

---

## 6. 포팅할 때 자주 틀리는 점

### 6.1 write/erase geometry 불일치

`StorageRegionConfig`의 `write_size`, `erase_size`는 backend의 실제 `WRITE_SIZE`, `ERASE_SIZE`와 맞아야 한다.
이 값이 다르면 mount 단계에서 실패할 수 있다.

### 6.2 embedded example을 실제 flash demo로 오해하기

현재 embedded examples는 `MockFlash`를 쓰므로 **하드웨어 flash persistence 데모가 아니다**.
빌드/구조 예제이자 smoke flow 예제다.

### 6.3 Linux example과 embedded example의 목적이 다름

- Linux example: std + file-backed persistence/reboot 확인
- embedded examples: no_std + Embassy + allocator-free smoke 확인

둘은 대체 관계가 아니라 역할이 다르다.

### 6.4 처음부터 TSDB 고급 기능까지 같이 붙이려 하기

처음에는 KV set/get, TS append/latest 확인까지만 붙이는 게 좋다.
그 다음에 아래를 늘리면 된다.
- `iter_by_time(...)`
- `query_count(...)`
- `set_status(...)`
- reboot/recovery 검증

---

## 7. 추천 시작 템플릿

### 7.1 host-side API 확인용
- `examples/linux/src/main.rs`
- 목적: API 사용법 + persistence/reopen 흐름 이해

### 7.2 Embassy 앱 골격 확인용
- `examples/stm32f401re/src/main.rs`
- `examples/nrf5340/src/main.rs`
- 목적: `no_std`, async main, smoke flow 골격 이해

### 7.3 실제 보드 포팅 순서
1. Linux example로 API 이해
2. embedded example으로 앱 골격 이식
3. `MockFlash`를 실제 `NorFlash` backend로 교체
4. geometry/region 맞춤
5. KV smoke -> TS smoke -> reboot/recovery 순으로 확대

---

## 8. 문서와 예제의 관계

관련 문서:
- `docs/linux-validation-procedure.md`
  - Linux host에서 canonical 검증 순서
- `docs/regression-test-catalog.md`
  - 회귀 테스트 레이어별 안내
- `docs/plans/07-testing-validation-and-rust-integration.md`
  - validation/integration 계획 문맥
- `docs/plans/07.5-no-std-no-alloc-transition.md`
  - no_std/no_alloc 방향과 embedded example 문맥

관련 example:
- `examples/linux/src/main.rs`
- `examples/stm32f401re/src/main.rs`
- `examples/nrf5340/src/main.rs`

---

## 9. 한 줄 요약

- **써보려면**: Linux example부터 실행한다.
- **포팅하려면**: embedded example 구조를 가져오고 `MockFlash`를 자기 보드의 `NorFlash` 구현으로 바꾼다.
- **검증하려면**: 마지막에는 `bash scripts/verify-all.sh` 기준으로 확인한다.
