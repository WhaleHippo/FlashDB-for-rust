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
- subprocess 기반 `flashdb-crash-harness`가 KV crash recovery, KV sector-header corruption recovery, TSDB reboot/query, TSDB PRE_WRITE tail recovery, TSDB partial-payload tail recovery, TSDB corrupted-index tail recovery, TSDB sector-header corruption recovery, TSDB status mutation reboot, TSDB deleted-status reboot, TSDB clean/reset reboot까지 검증한다.

## 2. 이번 작업: plan 07 다섯 번째 slice 중 TS partial-payload tail reboot recovery 추가

이번 작업의 목표는 `docs/plans/07-testing-validation-and-rust-integration.md`의 남은 corruption scenario 확장 중 TS payload partial write 조각을 실제 subprocess crash harness에 반영하는 것이었다.
이번 slice에서는 TSDB variable-blob 경로에서 “PRE_WRITE index는 이미 기록됐고 payload는 일부만 써진 상태에서 재부팅돼도, 이전 정상 레코드는 유지되고 이후 fresh append가 다시 가능해야 한다”를 구현/검증했다.

### 2.1 구현한 범위
추가한 동작은 다음과 같다.
- `tests/crash_scenarios.rs`
  - `tsdb_process_restart_recovers_from_partial_payload_tail` 신규 추가
- `src/bin/flashdb-crash-harness.rs`
  - `ts-inject-partial-payload-tail` 명령 추가
  - 기존 `TSL_PRE_WRITE` tail index를 만든 뒤 payload 영역에 일부 바이트만 실제로 쓰는 helper 추가

핵심 결과:
- reboot 뒤 기존 정상 timestamp `[10, 20]`는 계속 보인다.
- payload가 일부만 써진 PRE_WRITE tail은 live record로 노출되지 않는다.
- recovery 뒤 새 append가 성공하고 live record count도 다시 3으로 증가한다.

### 2.2 이번 slice의 해석
이번 작업도 plan 07의 corruption expansion 전체를 끝낸 것은 아니다.
현재까지 완료한 TS corruption slice는 다음과 같다.
- 완료: TS PRE_WRITE tail reboot recovery
- 완료: TS partial-payload tail reboot recovery
- 완료: TS corrupted index tail reboot recovery
- 완료: TS sector-header corruption reboot recovery
- 아직 남음: regression catalog 문서화

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
- TSDB reboot query / reboot append recovery 유지
- TSDB status mutation reboot / deleted-status reboot / clean reboot 유지

즉, 현재 구조는
- core: no_std + bounded no_alloc
- host validation: std-only file-backed simulator + subprocess crash harness
로 유지되며, 이번 작업으로 TSDB interrupted-write regression coverage가 더 촘촘해졌다.

## 4. 이번에 수정된 파일

### 코드
- `src/bin/flashdb-crash-harness.rs`

### 테스트
- `tests/crash_scenarios.rs`

### 문서
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업은 TDD 순서로 진행했다.

1. 먼저 신규 crash scenario test를 추가했다.
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_partial_payload_tail -- --exact`
- 초기에는 harness에 `ts-inject-partial-payload-tail` 명령이 없어 실패했다.

2. 이후 harness에 partial payload injection 경로를 구현했다.
- 구현 방식은 현재 Rust TSDB append 순서(`PRE_WRITE index -> payload write -> status transition`)를 그대로 반영해,
  먼저 PRE_WRITE tail index를 만들고 그 뒤 payload 영역에 일부 바이트만 기록하는 형태다.
- 이 slice에서는 core recovery 로직 추가 수정 없이도 기존 PRE_WRITE tail recovery 규칙이 partial payload case까지 커버함을 확인했다.

3. 구현 후 다음 검증을 모두 통과했다.
- `cargo fmt`
- `cargo test --features std --test crash_scenarios tsdb_process_restart_recovers_from_partial_payload_tail -- --exact`
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
  - reboot simulation 뒤 기존 append/query 결과가 유지되는 흐름
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - append 순서가 PRE_WRITE 상태의 index를 먼저 기록한 뒤 payload를 기록하고 마지막에 status를 진전시키는 구조
  - 따라서 payload partial write는 자연스럽게 PRE_WRITE tail의 하위 케이스로 볼 수 있다.

비교 요약:
- 공통점
  - interrupted write 이후에도 이전 정상 record는 유지되어야 한다.
  - 완료되지 않은 tail은 live record로 노출되면 안 된다.
- 차이점
  - 현재 Rust 구현은 file-backed subprocess harness에서 payload 일부 기록 상태를 직접 주입해 명시적으로 회귀 테스트로 고정했다.
  - recovery 자체는 별도 새 알고리즘보다는 기존 PRE_WRITE tail 처리 규칙을 재사용한다.

즉, upstream의 append/복구 철학을 따르되, 현재 Rust 쪽은 partial payload case를 subprocess 회귀 테스트로 분리해 “PRE_WRITE tail recovery가 실제 partial payload에도 적용된다”는 점을 명시적으로 검증했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.
- 이 카탈로그는 mock/file/Linux host validation 레이어 차이와 실행 순서를 함께 정리해야 한다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. regression catalog 정리
   - mock/file/Linux host validation 검증 레이어 차이와 실행 순서 문서화
2. 필요하면 이후 plan 07 완료 선언 전 문서 간 상호참조 정리
   - `docs/linux-validation-procedure.md`와 새 catalog 문서 링크 연결

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07 crash/reboot 검증에 TS partial-payload tail reboot recovery가 추가됐다. PRE_WRITE tail index 뒤 payload 일부만 써진 상태를 subprocess harness로 재현했고, 기존 PRE_WRITE recovery 규칙이 정상 record 유지 + fresh append 재개를 만족함을 검증했다. 다음은 regression catalog 문서화다."