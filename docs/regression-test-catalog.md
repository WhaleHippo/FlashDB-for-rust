# FlashDB-for-rust Regression Test Catalog

이 문서는 현재 저장소의 검증 레이어를 한 번에 파악할 수 있도록 테스트/스크립트/예제의 역할을 정리한 카탈로그다.

목적은 다음과 같다.
- 어떤 변경에서 어떤 검증부터 돌려야 하는지 빠르게 판단
- mock flash / file-backed / Linux host validation 레이어 차이를 명확히 유지
- crash/reboot 회귀 시나리오가 어느 파일에 고정되어 있는지 즉시 찾기

## 1. 권장 실행 순서

저장소 루트(`~/Desktop/FlashDB-for-rust`)에서 아래 순서로 실행한다.

1. foundation + core baseline
   - `cargo test`
2. std/file-backed 포함 전체 테스트
   - `cargo test --features std`
3. subprocess crash/reboot 집중 검증
   - `cargo test --features std --test crash_scenarios`
   - `bash scripts/run-crash-tests.sh`
4. Linux host smoke flow
   - `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
5. 전체 canonical verification
   - `bash scripts/verify-all.sh`

## 2. 검증 레이어 개요

### Layer 1. foundation / pure unit tests

목적:
- alignment, layout, status codec, CRC, blob 계층, region/config validation을 DB 정책과 분리해서 고정한다.

주요 파일:
- `tests/align.rs`
  - 기본 boundary align helper 검증
- `tests/aligned_write.rs`
  - aligned write passthrough / tail padding / NOR semantics / unaligned offset 거부
- `tests/status_codec.rs`
  - 1-bit, 1-byte, 4-byte, 8-byte status table roundtrip
  - partial programming 거부
  - incremental programming 바이트 패턴 검증
- `tests/layout_common.rs`
  - 공통 상수와 status count 기대값 검증
- `tests/layout_kv.rs`
  - KV layout 길이, sector header, padding rule, CRC seed, decode 최소 길이 검증
- `tests/layout_ts.rs`
  - TS sector header/index codec, dual-ended capacity helper, invalid time width 거부
- `tests/crc_compat.rs`
  - CRC32 known vector / chained CRC / FF padding 변화 검증
- `tests/blob_layer.rs`
  - `BlobRef`, `BlobBuf`, locator validation, chunk/cursor read 계약 검증
- `tests/blob_codec.rs`
  - typed value codec 계층의 encode/decode contract 검증
- `tests/storage_region.rs`
  - region validation / sector geometry 계산 검증
- `tests/storage_offset_map.rs`
  - logical offset ↔ absolute address 매핑 검증
- `tests/nor_flash_region_geometry.rs`
  - backend geometry 불일치 거부 검증
- `tests/config_validation.rs`
  - KV/TS config의 zero-limit, no_alloc cap, region bound 검증
- `tests/no_alloc_bounds.rs`
  - bounded no_alloc runtime cap 초과 시 write/append 거부 검증

언제 우선 실행하나:
- layout/status/header/alignment/blob/common helper 수정 시
- config/region/bounded-capacity 규칙 수정 시

### Layer 2. mock flash integration tests

목적:
- 빠른 반복 속도로 KVDB/TSDB 의미론, recovery, GC, rollover를 확인한다.
- 단일 프로세스 안에서 대부분의 동작 회귀를 고정한다.

주요 파일:
- `tests/kv_basic.rs`
  - set/get/delete/format
  - latest-wins overwrite
  - sector boundary crossing과 full 상태 no-space 반환
- `tests/kv_recovery.rs`
  - PRE_WRITE tail discard
  - CRC-mismatched tail discard
  - corrupted next-sector header recovery + reuse
- `tests/kv_plan05.rs`
  - sector metadata(store/dirty/remaining bytes)
  - PRE_DELETE-aware recovery
  - live traversal / integrity check
  - overwrite 누적 후 GC / manual GC / iterator snapshot
- `tests/ts_basic.rs`
  - variable-blob append
  - strict monotonic timestamp policy
  - reverse iteration
  - inclusive `iter_by_time` / `query_count`
  - status mutation
  - clean/reset + reboot reuse
- `tests/ts_rollover.rs`
  - rollover off/on
  - oldest/current/live record 유지
  - fixed-blob mode append/iterate/reboot

언제 우선 실행하나:
- KVDB/TSDB core logic 변경 시
- GC, recovery, iterator, query, status transition 수정 시

### Layer 3. std file-backed simulation tests

목적:
- 메모리 mock이 아닌 실제 file-backed `NorFlash` 경로를 사용해 reboot persistence를 검증한다.
- 같은 프로세스 내부 reopen과 프로세스 경계 reboot를 구분해 다룬다.

주요 파일:
- `tests/file_sim.rs`
  - file-backed simulator reopen 뒤 KV/TSDB state 유지
  - erase-before-rewrite NOR semantics 강제
- `src/storage/file_sim.rs`
  - std-only backing file 기반 `NorFlash` 구현

언제 우선 실행하나:
- file-backed backend 수정 시
- std feature storage/reopen/persistence 수정 시

### Layer 4. subprocess crash/reboot simulation

목적:
- 서로 다른 프로세스가 동일 backing file을 순차적으로 열며 power-loss 유사 회귀를 고정한다.
- interrupted tail, corruption, reboot-after-status-mutation 같은 시나리오를 명시적 테스트 이름으로 유지한다.

핵심 파일:
- `tests/crash_scenarios.rs`
- `src/bin/flashdb-crash-harness.rs`
- `scripts/run-crash-tests.sh`

현재 고정된 시나리오:
- KV
  - `kv_process_restart_recovers_from_pre_write_tail`
  - `kv_process_restart_recovers_from_crc_mismatched_tail`
  - `kv_process_restart_recovers_from_corrupted_next_sector_header`
- TSDB
  - `tsdb_process_restart_recovers_from_pre_write_tail`
  - `tsdb_process_restart_preserves_query_and_reverse_iteration_after_reopen`
  - `tsdb_process_restart_preserves_status_mutation_across_reopen`
  - `tsdb_process_restart_preserves_clean_reset_across_reopen`
  - `tsdb_process_restart_preserves_deleted_status_across_reopen`
  - `tsdb_process_restart_recovers_from_corrupted_index_tail`
  - `tsdb_process_restart_recovers_from_corrupted_next_sector_header`
  - `tsdb_process_restart_recovers_from_partial_payload_tail`

시나리오 해석 요약:
- PRE_WRITE tail: 완료되지 않은 tail이 live record로 드러나지 않아야 함
- CRC mismatch / corrupted index tail: 손상된 tail이 mount/query를 깨지 않아야 함
- corrupted next-sector header: 손상된 다음 sector를 버리거나 복구해도 이전 정상 데이터와 fresh write가 유지되어야 함
- query/reverse iteration reboot: remount 뒤 range/query/역순 순회 결과가 유지되어야 함
- status/deleted/clean reboot: 상태 테이블 변경과 clean/reset 결과가 프로세스 재시작 뒤에도 일관돼야 함
- partial payload tail: PRE_WRITE index 뒤 payload가 일부만 기록된 상태에서도 이전 정상 record 유지 + fresh append 재개가 가능해야 함

언제 우선 실행하나:
- crash harness 수정 시
- mount/recovery/corruption handling 수정 시
- file-backed persistence와 status transition 상호작용 수정 시

## 3. Linux host validation / examples

목적:
- 사용자가 실제로 따라 하는 host-side persistence 흐름을 smoke 수준으로 고정한다.

주요 경로:
- `examples/linux/src/bin/flashdb.rs`
  - std feature + file-backed backend를 사용하는 host example
- `docs/linux-validation-procedure.md`
  - canonical 실행 순서와 확인 포인트 문서
- `scripts/verify-all.sh`
  - repo 전체 검증 스크립트

`bash scripts/verify-all.sh`가 수행하는 핵심 검증:
- root / example crate `cargo fmt --check`
- `src/`에 `extern crate alloc` / `alloc::` 잔존 여부 검사
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`

언제 우선 실행하나:
- release 전 최종 확인
- 문서/예제/embedded smoke build까지 함께 확인해야 할 때
- no_alloc invariant와 host validation을 한 번에 보장하고 싶을 때

## 4. 변경 유형별 추천 검증 매트릭스

### A. layout / codec / alignment / region 수정
- 최소:
  - `cargo test --test status_codec`
  - `cargo test --test layout_kv`
  - `cargo test --test layout_ts`
  - `cargo test --test aligned_write`
  - `cargo test --test blob_layer`
- 권장 마무리:
  - `cargo test`

### B. KVDB core / recovery / GC 수정
- 최소:
  - `cargo test --test kv_basic`
  - `cargo test --test kv_recovery`
  - `cargo test --test kv_plan05`
- reboot/file-backed 영향이 있으면 추가:
  - `cargo test --features std --test crash_scenarios`
  - `bash scripts/run-crash-tests.sh`

### C. TSDB append / query / status / rollover 수정
- 최소:
  - `cargo test --test ts_basic`
  - `cargo test --test ts_rollover`
- reboot/file-backed 영향이 있으면 추가:
  - `cargo test --features std --test crash_scenarios`
  - `bash scripts/run-crash-tests.sh`

### D. std file-backed backend / crash harness 수정
- 최소:
  - `cargo test --features std --test file_sim`
  - `cargo test --features std --test crash_scenarios`
  - `bash scripts/run-crash-tests.sh`
- 권장 마무리:
  - `cargo test --features std`
  - `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`

### E. examples / docs / release-candidate 확인
- 최소:
  - `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- 권장 마무리:
  - `bash scripts/verify-all.sh`

## 5. 문제 발생 시 먼저 볼 파일

### crash/reboot 실패
- `tests/crash_scenarios.rs`
- `src/bin/flashdb-crash-harness.rs`
- `src/storage/file_sim.rs`

### mock integration 실패
- `tests/kv_basic.rs`
- `tests/kv_recovery.rs`
- `tests/kv_plan05.rs`
- `tests/ts_basic.rs`
- `tests/ts_rollover.rs`

### foundation/layout 실패
- `tests/status_codec.rs`
- `tests/layout_kv.rs`
- `tests/layout_ts.rs`
- `tests/aligned_write.rs`
- `tests/blob_layer.rs`
- `tests/config_validation.rs`

## 6. 문서 관계

- 상위 계획: `docs/plans/07-testing-validation-and-rust-integration.md`
- 진행 snapshot: `docs/plans/progress.md`
- canonical host 실행 절차: `docs/linux-validation-procedure.md`
