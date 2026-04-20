# FlashDB-for-rust 진행 현황

작성일: 2026-04-20
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 진행 기준: `docs/plans/07-testing-validation-and-rust-integration.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 완료
  - plan 06: 완료
  - plan 07: 5차 crash/reboot simulation slice 진행 중
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- subprocess 기반 `flashdb-crash-harness`가 KV crash recovery, KV sector-header corruption recovery, TSDB reboot/query, TSDB PRE_WRITE tail recovery, TSDB corrupted-index tail recovery, TSDB sector-header corruption recovery, TSDB status mutation reboot, TSDB deleted-status reboot, TSDB clean/reset reboot까지 검증한다.

## 2. 이번 작업: plan 07 다섯 번째 slice 중 TS sector-header corruption reboot recovery 추가

이번 작업의 목표는 `docs/plans/07-testing-validation-and-rust-integration.md`의 남은 corruption scenario 확장 중 다음 조각을 실제 코드와 subprocess crash harness에 반영하는 것이었다.
이번 slice에서는 TSDB variable-blob 경로에서 “두 번째 sector header가 손상된 뒤 재부팅해도 이전 정상 sector의 레코드는 유지되고, 손상된 sector는 복구 후 다시 append 대상으로 재사용 가능해야 한다”를 구현/검증했다.

### 2.1 구현한 범위
추가한 동작은 다음과 같다.
- `tests/crash_scenarios.rs`
  - `tsdb_process_restart_recovers_from_corrupted_next_sector_header` 신규 추가
- `src/bin/flashdb-crash-harness.rs`
  - `ts-init-two-sector-fill` 명령 추가
  - `ts-corrupt-next-sector-header` 명령 추가
  - TS sector magic을 직접 손상시키는 helper 추가
  - 복구 뒤 surviving timestamps와 fresh append 재개를 확인하는 `ts-check-corrupted-sector-recovery` 명령 추가
- `src/tsdb/db.rs`
  - mount-time sector scan에서 손상된 TS sector header decode 실패 시 전체 mount를 실패시키지 않고 해당 sector를 erase 후 empty/reusable sector로 복구하도록 보강

핵심 결과:
- reboot 뒤 손상되지 않은 첫 sector의 timestamp `[10, 20]`는 계속 보인다.
- 손상된 두 번째 sector는 mount 중 erase되어 재사용 가능 상태가 된다.
- recovery 뒤 timestamp `50` append가 다시 성공해 최종 live timestamps가 `[10, 20, 50]`가 된다.

### 2.2 이번 slice의 해석
이번 작업도 plan 07의 corruption expansion 전체를 끝낸 것은 아니다.
현재까지 완료한 TS corruption slice는 다음과 같다.
- 완료: TS corrupted index tail reboot recovery
- 완료: TS sector-header corruption reboot recovery
- 아직 남음: TS payload partial write 시나리오, regression catalog 문서화

## 3. 기존 완료 상태 유지

이전 세션까지 완료된 plan 07 / 07.5 결과는 그대로 유지된다.
- core `src/`에서 `extern crate alloc` 제거
- `src/kv/*`, `src/tsdb/*`의 `alloc::vec`, `alloc::string` 제거
- 동적 할당 대신 `heapless` 기반 bounded container 사용
- `src/config.rs`의 bounded no_alloc cap 검증 유지
- allocator 없는 embedded smoke example 유지
- `src/storage/file_sim.rs`의 std-only file-backed backend 유지
- `examples/linux/src/bin/flashdb.rs`의 file-backed smoke example 유지
- KV PRE_WRITE / CRC tail의 subprocess crash recovery test 유지
- KV corrupted next-sector header recovery 유지
- TSDB PRE_WRITE tail / reboot query / reboot append recovery 유지
- TSDB status mutation reboot / deleted-status reboot / clean reboot 유지

즉, 현재 구조는
- core: no_std + bounded no_alloc
- host validation: std-only file-backed simulator + subprocess crash harness
로 유지되며, 이번 작업으로 TSDB corruption regression coverage가 또 한 단계 더 넓어졌다.

## 4. 이번에 수정된 파일

### 코드
- `src/bin/flashdb-crash-harness.rs`
- `src/tsdb/db.rs`

### 테스트
- `tests/crash_scenarios.rs`

### 문서
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업은 TDD 순서로 진행했다.

1. 먼저 신규 crash scenario test를 추가했다.
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_corrupted_next_sector_header -- --exact`
- 초기에는 harness에 `ts-init-two-sector-fill` / `ts-corrupt-next-sector-header` 명령이 없어 실패했다.

2. 이후 harness 명령과 TSDB sector-header recovery 로직을 구현했다.
- 첫 구현에서는 mount가 손상 sector를 비어 있는 sector처럼 취급했지만, 실제 flash 내용이 erase되지 않아 append 시 `Storage(RequiresErase)`가 발생했다.
- 원인을 확인한 뒤 mount-time scan에서 손상 sector header를 발견하면 해당 sector를 erase하고 reusable empty sector로 복구하도록 수정했다.

3. 구현 후 다음 검증을 모두 통과했다.
- `cargo fmt`
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_corrupted_next_sector_header -- --exact`
- `bash scripts/run-crash-tests.sh`
- `bash scripts/verify-all.sh`

`bash scripts/verify-all.sh` 안에서 추가 확인된 항목:
- `cargo fmt --check`
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`

## 6. upstream 비교 메모

실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
  - `fdb_reboot()` 기반으로 init/deinit 뒤 query/iter/status/clean semantics를 다시 확인하는 흐름
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - `check_sec_hdr_cb(...)`가 sector header 검사 결과를 바탕으로 후속 recovery/format 흐름을 결정하는 구조
  - 잘못된 sector header가 있으면 전체 체크 실패로 간주하고 format 경로로 넘기는 보수적 접근

비교 요약:
- 공통점
  - 손상된 sector header를 정상 live sector로 계속 신뢰하지 않는 복구 지향 철학은 같다.
  - reboot 뒤에도 무결한 sector의 데이터가 계속 읽혀야 한다는 목표는 같다.
- 차이점
  - upstream C는 header check 실패 시 더 큰 단위의 format 경로로 정리하는 보수적 모델에 가깝다.
  - 현재 Rust 구현은 std file-backed subprocess harness에서 “손상된 sector만 erase 후 재사용하고, 나머지 정상 sector는 유지”하는 more-local recovery를 택했다.
  - 즉, 현재 Rust slice는 exact upstream parity보다는 correctness-first의 국소 복구 전략이다.

즉, upstream의 sector-header validation 철학은 참고하되, 현재 Rust 구조에서는 손상된 TS sector header를 mount 시점에 해당 sector만 erase하여 정상 sector 보존 + fresh append 재개를 만족시키는 pragmatic recovery를 택했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. TS corruption scenario 추가 확대
- 현재 subprocess crash regression은
  - KV PRE_WRITE tail
  - KV CRC mismatch tail
  - KV corrupted next-sector header
  - TSDB PRE_WRITE tail
  - TSDB corrupted index tail
  - TSDB corrupted next-sector header
  - TSDB reboot 후 query/iteration
  - TSDB status mutation reboot
  - TSDB deleted-status reboot
  - TSDB clean/reset reboot
  까지 커버한다.
- 이후 남은 가장 좋은 다음 조각은
  - TS payload partial write
  같은 시나리오다.

2. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.
- 이 카탈로그는 mock/file/Linux host validation 레이어 차이와 실행 순서를 함께 정리해야 한다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 다섯 번째 slice 계속 진행
   - TS payload partial write subprocess scenario 추가
2. 그 다음 regression catalog 정리
   - mock/file/Linux host validation 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07 crash/reboot 검증에 TS sector-header corruption reboot recovery가 추가됐다. mount 시 손상된 TS sector만 erase해 정상 sector 데이터는 유지하고 fresh append를 재개한다. 다음은 TS payload partial write와 regression catalog 문서화다."