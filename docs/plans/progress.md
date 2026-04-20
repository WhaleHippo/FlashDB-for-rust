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
- subprocess 기반 `flashdb-crash-harness`가 KV crash recovery, KV sector-header corruption recovery, TSDB reboot/query, TSDB PRE_WRITE tail recovery, TSDB corrupted-index tail recovery, TSDB status mutation reboot, TSDB deleted-status reboot, TSDB clean/reset reboot까지 검증한다.

## 2. 이번 작업: plan 07 다섯 번째 slice 중 TS corrupted-index tail reboot recovery 추가

이번 작업의 목표는 `docs/plans/07-testing-validation-and-rust-integration.md`의 남은 corruption scenario 확장 중 하나를 실제 코드와 subprocess crash harness에 반영하는 것이었다.
이번 slice에서는 TSDB variable-blob 경로에서 “손상된 index tail이 남은 뒤 재부팅해도 이전 정상 레코드는 유지되고, 이후 fresh append가 다시 가능해야 한다”를 구현/검증했다.

### 2.1 구현한 범위
추가한 동작은 다음과 같다.
- `tests/crash_scenarios.rs`
  - `tsdb_process_restart_recovers_from_corrupted_index_tail` 신규 추가
- `src/bin/flashdb-crash-harness.rs`
  - `ts-inject-corrupted-index-tail` 명령 추가
  - 현재 writable TS sector의 tail index slot에 out-of-bounds `log_addr`를 가진 손상 index를 주입하는 helper 추가
- `src/tsdb/db.rs`
  - mount-time sector scan에서 variable-mode TS corrupted tail index를 fatal error로 종료하지 않고 dead tail slot으로 건너뛰도록 보강
  - iteration/query/status-lookup 경로에서도 같은 corrupted tail slot을 건너뛰도록 보강

핵심 결과:
- reboot 뒤 기존 정상 timestamp `[10, 20]`는 계속 보인다.
- 손상 index tail은 live record로 노출되지 않는다.
- recovery 뒤 새 append가 성공하고 live record count도 다시 증가한다.

### 2.2 이번 slice의 해석
이번 작업은 plan 07의 corruption expansion 전체를 끝낸 것이 아니다.
현재는 다음 중 한 조각만 완료했다.
- 완료: TS corrupted index tail reboot recovery
- 아직 남음: TS payload partial write 시나리오, TS sector-header corruption 시나리오, regression catalog 문서화

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
로 유지되며, 이번 작업으로 TSDB corruption regression coverage가 한 단계 더 넓어졌다.

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
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_corrupted_index_tail -- --exact`
- 초기에는 harness에 `ts-inject-corrupted-index-tail` 명령이 없어 실패했다.

2. 이후 harness + TSDB recovery 로직을 구현했다.

3. 구현 후 다음 검증을 모두 통과했다.
- `cargo fmt`
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_corrupted_index_tail -- --exact`
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
  - `read_tsl(...)`로 TSL 상태를 읽고 `FDB_TSL_PRE_WRITE`를 무시하는 scan 흐름
  - sector header magic / TS index의 `log_addr`를 storage metadata로 해석하는 구조

비교 요약:
- 공통점
  - reboot 뒤에도 scan 결과가 이전 정상 record를 계속 가리켜야 한다는 철학은 같다.
  - tail 쪽의 미완성/비정상 엔트리가 전체 mount를 무너뜨리면 안 된다는 복구 지향 흐름을 유지한다.
- 차이점
  - upstream C는 주로 같은 프로세스 안에서 reboot simulation을 반복한다.
  - 현재 Rust 구현은 std file-backed backend + subprocess harness를 써서 실제 프로세스 경계를 넘는 reboot 회귀를 검증한다.
  - 이번 slice의 corrupted index recovery는 upstream의 exact end-info / scanner 구현을 그대로 옮긴 것은 아니고, 현재 Rust 구조에 맞춰 variable-mode tail slot을 dead slot으로 건너뛰는 pragmatic recovery 방식이다.

즉, upstream의 reboot recovery 철학은 따르되, 현재 Rust 구조에서는 corrupted variable tail index를 mount/iter/query에서 skip하는 방식으로 correctness-first 복구를 택했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. TS corruption scenario 추가 확대
- 현재 subprocess crash regression은
  - KV PRE_WRITE tail
  - KV CRC mismatch tail
  - KV corrupted next-sector header
  - TSDB PRE_WRITE tail
  - TSDB corrupted index tail
  - TSDB reboot 후 query/iteration
  - TSDB status mutation reboot
  - TSDB deleted-status reboot
  - TSDB clean/reset reboot
  까지 커버한다.
- 이후 남은 좋은 다음 조각은
  - TS payload partial write
  - TS sector-header corruption
  같은 시나리오다.

2. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.
- 이 카탈로그는 mock/file/Linux host validation 레이어 차이와 실행 순서를 함께 정리해야 한다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 다섯 번째 slice 계속 진행
   - TS sector-header corruption 또는 TS payload partial write subprocess scenario 추가
2. 그 다음 regression catalog 정리
   - mock/file/Linux host validation 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07 crash/reboot 검증에 TS corrupted-index tail reboot recovery가 추가됐다. std file-backed subprocess harness가 손상 tail을 dead slot으로 건너뛰고 정상 record 유지 + fresh append 재개를 검증한다. 다음은 TS sector-header corruption 또는 payload partial write, 그리고 regression catalog 문서화다."