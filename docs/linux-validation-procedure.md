# FlashDB-for-rust Linux Validation Procedure

이 문서는 FlashDB-for-rust를 Linux host에서 반복 가능하게 검증하는 canonical 절차를 정리한다.

## 목적

STM32F302 hardware smoke 방향은 폐기하고, 현재 프로젝트의 기본 검증 축을 Linux host 기반 persistence/recovery 검증으로 둔다.

이 절차의 목표는 다음과 같다.
- 빠른 반복 검증
- subprocess/file-backed reboot 재현
- std feature 기반 실제 persistence 확인
- 다음 세션에서도 동일한 순서로 재실행 가능한 canonical flow 제공

## 전제

- 저장소 루트: `~/Desktop/FlashDB-for-rust`
- Linux host에서 Rust toolchain이 정상 설치되어 있음
- `std` feature와 file-backed simulator가 사용 가능함

## 권장 실행 순서

저장소 루트에서 아래 순서로 실행한다.

1. 기본 테스트
   - `cargo test`

2. std feature 테스트
   - `cargo test --features std`

3. subprocess crash/reboot 시나리오 집중 검증
   - `cargo test --features std --test crash_scenarios`
   - `bash scripts/run-crash-tests.sh`

4. Linux host smoke example 실행
   - `cargo run --manifest-path examples/linux/Cargo.toml`

5. 전체 검증 스크립트
   - `bash scripts/verify-all.sh`

## 확인 포인트

### 1. unit / integration baseline
- KVDB 기본 set/get/delete/overwrite가 깨지지 않는가
- TSDB append/query/count/iter semantics가 유지되는가

### 2. reboot / crash recovery
- file-backed simulator에서 프로세스 재시작 뒤 mount가 정상인가
- PRE_WRITE tail이 남아도 이전 live data가 유지되는가
- CRC mismatch 또는 corruption 뒤에도 recovery가 가능한가

### 3. Linux example smoke
- host example이 실제 std/file-backed flow를 사용하고 있는가
- fresh run 이후 재실행했을 때 persistence 경로가 드러나는가

## 현재 중점 회귀 시나리오

현재 subprocess crash harness는 최소 다음 시나리오를 커버해야 한다.
- KV PRE_WRITE tail recovery
- KV CRC mismatch tail recovery
- KV corrupted next-sector header recovery
- TSDB PRE_WRITE tail recovery
- TSDB reboot 후 query/iteration 유지
- TSDB status mutation reboot
- TSDB deleted-status reboot
- TSDB clean/reset reboot
- TSDB partial-payload tail recovery
- TSDB corrupted index tail recovery
- TSDB sector-header corruption recovery

세부 테스트 이름과 레이어별 실행 매트릭스는 `docs/regression-test-catalog.md`를 기준으로 본다.

## 실패 시 먼저 볼 것

- `tests/crash_scenarios.rs`
- `src/bin/flashdb-crash-harness.rs`
- `src/storage/file_sim.rs`
- `scripts/run-crash-tests.sh`
- `scripts/verify-all.sh`

## 문서 관계

- 상위 계획: `docs/plans/07-testing-validation-and-rust-integration.md`
- 진행 snapshot: `docs/plans/progress.md`
- 회귀 카탈로그: `docs/regression-test-catalog.md`
