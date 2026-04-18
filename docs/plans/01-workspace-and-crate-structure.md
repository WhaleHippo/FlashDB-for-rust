# Workspace and Crate Structure Plan

> 목적: 구현이 시작되기 전에 crate 구조, 모듈 책임, 네이밍 규칙, public/internal 경계를 고정한다.

## 1. 목표

이 문서의 목표는 “파일 어디에 무엇을 넣을지”를 명확히 정하는 것이다.
FlashDB-for-rust는 초기에 코드 양보다 구조가 더 중요하다.
crate 구조가 불안정하면 recovery/GC/TSDB를 붙일수록 복잡도가 폭발한다.

## 2. 설계 원칙

### 2.1 public API는 얇게
외부 사용자는 가급적 아래 몇 개 레벨만 보도록 한다.
- `config`
- `error`
- `blob`
- `kv`
- `tsdb`
- `storage` 중 필요한 최소 타입

### 2.2 internal byte layout은 별도 모듈
- byte encoding/decoding과 DB 정책을 분리한다.
- 온플래시 헤더 해석은 `layout/*`에만 둔다.

### 2.3 backend 독립성 유지
- flash primitive는 `storage/*`에 모은다.
- KVDB/TSDB는 특정 HAL 타입을 직접 알지 않게 한다.

### 2.4 no_std 우선
- 기본 crate는 `#![no_std]`
- 테스트 또는 file simulator에서만 `std` 허용

## 3. 제안 디렉터리 구조

```text
src/
├─ lib.rs
├─ error.rs
├─ config.rs
├─ crc.rs
├─ storage/
├─ layout/
├─ blob/
├─ kv/
└─ tsdb/
```

## 4. 파일별 책임 계획

### 4.1 `src/lib.rs`

역할:
- crate entrypoint
- public re-export
- feature gate 정리

초기 export 권장:
- `pub mod error;`
- `pub mod config;`
- `pub mod storage;`
- `pub mod blob;`
- `pub mod kv;`
- `pub mod tsdb;`

주의:
- `layout`는 처음엔 internal 모듈로 두는 편이 낫다.
- low-level layout 타입을 public에 너무 빨리 노출하지 않는다.

### 4.2 `src/error.rs`

역할:
- crate 공통 error 정의

초기 enum 후보:
- `StorageError`
- `DecodeError`
- `AlignmentError`
- `OutOfBounds`
- `CorruptedHeader`
- `CrcMismatch`
- `NoSpace`
- `UnsupportedFormatVersion`
- `InvariantViolation`
- `TimestampNotMonotonic`

가이드:
- storage backend error를 wrapping할지, adapter에서 내부 변환할지 초기 설계에서 결정
- `core::fmt::Display`는 나중에 붙여도 되지만, debug 정보는 충분해야 함

### 4.3 `src/config.rs`

역할:
- runtime/static configuration 타입

예상 타입:
- `StorageRegionConfig`
- `KvConfig`
- `TsdbConfig`
- `BlobMode`
- `TimestampPolicy`

원칙:
- C 매크로를 feature flag/const/runtime config로 나누어 이식한다.
- config가 layout에 영향을 주는 항목은 format version과 함께 관리 가능하도록 설계한다.

### 4.4 `src/crc.rs`

역할:
- CRC32 계산 전담

가이드:
- 원본과 동일한 CRC32 결과를 재현할 것
- padding 포함 계산이 필요한 지점을 helper로 분리할 것

### 4.5 `src/storage/`

하위 파일 계획:
- `mod.rs`
- `region.rs`
- `nor_flash.rs`
- `mock.rs`
- `file_sim.rs` (std feature 또는 tests 전용)

#### `region.rs`
- flash 전체가 아니라 특정 DB 영역만 표현
- `start`, `len`, `erase_size`, `write_size` 불변식 보장

#### `nor_flash.rs`
- `embedded_storage::nor_flash::{ReadNorFlash, NorFlash}` adapter
- logical offset과 backend offset 변환

#### `mock.rs`
- RAM 기반 mock flash
- NOR semantics 보장: write 시 1->0만 허용

#### `file_sim.rs`
- host-side crash/reboot 재현용
- 필요 시 `cfg(feature = "std")`

### 4.6 `src/layout/`

하위 파일 계획:
- `mod.rs`
- `align.rs`
- `status.rs`
- `common.rs`
- `kv.rs`
- `ts.rs`

#### `align.rs`
- `align_up`
- `align_down`
- `write_size` 경계 helper

#### `status.rs`
- status table encode/decode
- write granularity별 상태 표현 helper

#### `common.rs`
- erased byte
- magic constants
- 공통 header field helper
- format version 상수

#### `kv.rs`
- KV sector header codec
- KV record header codec
- CRC 범위 helper

#### `ts.rs`
- TS sector header codec
- TS index codec
- fixed/variable blob mode 계산 helper

원칙:
- `layout/*`는 비즈니스 로직을 몰라야 한다.
- decode는 validation-aware이어야 하되, policy는 몰라야 한다.

### 4.7 `src/blob/`

하위 파일 계획:
- `mod.rs`
- `locator.rs`
- `reader.rs`
- `codec.rs`

이 모듈은 원본 `fdb_blob`의 역할 분해를 담당한다.

### 4.8 `src/kv/`

하위 파일 계획:
- `mod.rs`
- `db.rs`
- `scan.rs`
- `write.rs`
- `recovery.rs`
- `gc.rs`
- `iter.rs`

권장 분리:
- `db.rs`: public API와 state struct
- `scan.rs`: mount/scan/find
- `write.rs`: set/delete/new record
- `recovery.rs`: PRE_WRITE/PRE_DELETE 복구
- `gc.rs`: dirty tracking과 copy-forward GC
- `iter.rs`: iterator API

### 4.9 `src/tsdb/`

하위 파일 계획:
- `mod.rs`
- `db.rs`
- `append.rs`
- `query.rs`
- `recovery.rs`
- `iter.rs`

권장 분리:
- `db.rs`: public API와 state
- `append.rs`: sector 상태 전이와 append
- `query.rs`: count, time-range query
- `recovery.rs`: current/oldest sector mount logic
- `iter.rs`: forward/reverse/range iteration

## 5. 테스트 디렉터리 구조 계획

```text
tests/
├─ status_codec.rs
├─ layout_kv.rs
├─ layout_ts.rs
├─ blob_layer.rs
├─ kv_basic.rs
├─ kv_recovery.rs
├─ kv_gc.rs
├─ ts_basic.rs
├─ ts_query.rs
└─ crash_scenarios.rs
```

원칙:
- 모듈 단위 unit test + 시나리오 integration test를 함께 유지
- crash test는 별도 파일로 분리

## 6. 구현 단계별 파일 생성 순서

1. `lib.rs`, `error.rs`, `config.rs`, `crc.rs`
2. `storage/mod.rs`, `storage/region.rs`, `storage/mock.rs`
3. `layout/mod.rs`, `layout/align.rs`, `layout/status.rs`, `layout/common.rs`
4. `layout/kv.rs`, `layout/ts.rs`
5. `blob/*`
6. `kv/*`
7. `tsdb/*`
8. tests / examples

## 7. 방법론

### 7.1 파일은 비어 있어도 먼저 자리부터 만든다
- 복잡한 모듈은 placeholder로 파일 구조를 먼저 만든다.
- import graph를 안정화한 뒤 세부 구현을 채운다.

### 7.2 순환 의존을 금지한다
금지 목표:
- `kv -> tsdb`
- `tsdb -> kv`
- `layout -> kv`
- `layout -> tsdb`

허용 방향:
- `kv -> layout`
- `tsdb -> layout`
- `blob -> storage/layout` 일부
- `storage`는 최하단

### 7.3 public API는 늦게 확정한다
- 먼저 internal API를 안정화한다.
- public export는 tests와 examples가 조금 생긴 뒤 다듬는다.

## 8. 참고 자료

### 우선 참고
- `docs/flashdb-architecture-analysis.md`
- `docs/plans/00-top-down-roadmap.md`

### 원본 코드 매핑 참고
- `~/Desktop/FlashDB/inc/flashdb.h`
- `~/Desktop/FlashDB/inc/fdb_def.h`
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
- `~/Desktop/FlashDB/src/fdb_tsdb.c`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/inc/flashdb.h`
  - public API surface가 어떻게 나뉘는지, Rust crate의 public re-export 경계를 잡을 때 참고한다.
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - `fdb_db`, `fdb_kvdb`, `fdb_tsdb`, `fdb_blob` 등의 구조를 보며 Rust 모듈 분리를 설계할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - KV 관련 세부 구현이 한 파일에 몰려 있는 현재 C 구조를 보고, Rust에서는 어떤 책임을 분리할지 판단할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - TSDB 구현의 경계와 KVDB와의 공통점/차이점을 비교하면서 Rust 모듈 경계를 잡을 때 참고한다.

## 9. 완료 기준

이 문서 기준으로 다음이 가능해야 한다.
- 새 task에서 “어느 파일을 만들어야 하지?”라는 질문이 거의 없어야 함
- 공통 모듈과 KV/TS 전용 모듈의 책임이 명확해야 함
- Blob, layout, storage가 뒤섞이지 않아야 함

## 10. 다음 문서

- `02-storage-layout-and-status-foundation.md`
- `03-blob-and-codec-layer.md`
