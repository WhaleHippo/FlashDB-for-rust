# FlashDB-for-rust 진행 현황

작성일: 2026-04-25
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
  - plan 07: 완료
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- subprocess 기반 `flashdb-crash-harness`가 KV crash recovery, KV sector-header corruption recovery, TSDB reboot/query, TSDB PRE_WRITE tail recovery, TSDB partial-payload tail recovery, TSDB corrupted-index tail recovery, TSDB sector-header corruption recovery, TSDB status mutation reboot, TSDB deleted-status reboot, TSDB clean/reset reboot까지 검증한다.
- Linux host canonical procedure 문서(`docs/linux-validation-procedure.md`)와 regression catalog(`docs/regression-test-catalog.md`)가 연결돼 있다.

## 2. 이번 작업: example 엔트리포인트 구조 정리 (`src/bin/flashdb.rs` -> `src/main.rs`)

이번 작업의 목표는 `examples/` 아래 각 플랫폼 예제 크레이트가 단일 예제 실행 파일을 더 직관적으로 다루도록 구조를 단순화하는 것이었다.
기존에는 Linux / STM32F401RE / nRF5340 예제가 모두 `src/bin/flashdb.rs`를 사용하고 있었는데, 이번 slice에서는 단일 바이너리 크레이트에 맞게 모두 `src/main.rs`로 옮겼다.

### 2.1 구현한 범위
- 예제 엔트리포인트 이동
  - `examples/linux/src/bin/flashdb.rs` -> `examples/linux/src/main.rs`
  - `examples/stm32f401re/src/bin/flashdb.rs` -> `examples/stm32f401re/src/main.rs`
  - `examples/nrf5340/src/bin/flashdb.rs` -> `examples/nrf5340/src/main.rs`
- 관련 문서/검증 경로 동기화
  - `scripts/verify-all.sh`에서 `--bin flashdb` 없이 실행/빌드하도록 갱신
  - `docs/plans/07-testing-validation-and-rust-integration.md` 경로 표기 갱신
  - `docs/plans/07.5-no-std-no-alloc-transition.md` 경로 표기 갱신
  - `docs/linux-validation-procedure.md`와 `docs/regression-test-catalog.md`의 예제 실행 명령 갱신

### 2.2 이번 slice의 해석
이번 변경은 알고리즘 변화가 아니라 example crate UX 정리다.
- 각 예제 크레이트가 단일 바이너리라는 의도가 `src/main.rs` 구조로 더 직접적으로 드러난다.
- Linux host 예제는 이제 `cargo run --manifest-path examples/linux/Cargo.toml`만으로 바로 실행된다.
- embedded smoke example도 `--bin flashdb` 지정 없이 바로 target build 검증이 가능하다.

plan 07 / 07.5의 완료 상태는 그대로 유지되며, 이번 slice는 그 위에 example 사용성과 문서 일관성을 정리한 후속 housekeeping 작업으로 본다.

## 3. 기존 완료 상태 유지

이전 세션까지 완료된 plan 07 / 07.5 결과는 그대로 유지된다.
- core `src/`에서 `extern crate alloc` 제거
- `src/kv/*`, `src/tsdb/*`의 `alloc::vec`, `alloc::string` 제거
- 동적 할당 대신 `heapless` 기반 bounded container 사용
- `src/config.rs`의 bounded no_alloc cap 검증 유지
- allocator 없는 embedded smoke example 유지
- `src/storage/file_sim.rs`의 std-only file-backed backend 유지
- `examples/linux/src/main.rs`의 file-backed smoke example 유지
- KV PRE_WRITE / CRC tail의 subprocess crash recovery test 유지
- KV corrupted next-sector header recovery 유지
- TSDB reboot query / reboot append recovery 유지
- TSDB status mutation reboot / deleted-status reboot / clean reboot 유지
- TSDB partial payload / corrupted index / sector-header corruption reboot recovery 유지

## 4. 이번에 수정된 파일

### 예제 엔트리포인트
- `examples/linux/src/main.rs`
- `examples/stm32f401re/src/main.rs`
- `examples/nrf5340/src/main.rs`

### 문서 / 검증 스크립트
- `scripts/verify-all.sh`
- `docs/linux-validation-procedure.md`
- `docs/regression-test-catalog.md`
- `docs/plans/07-testing-validation-and-rust-integration.md`
- `docs/plans/07.5-no-std-no-alloc-transition.md`
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업은 example 엔트리포인트 구조와 문서/스크립트 참조를 정리하는 slice였고, 구조 변경 후 기존 검증 파이프라인이 그대로 통과하는지 확인하는 방향으로 검증했다.

통과한 검증:
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --target thumbv8m.main-none-eabihf`
- `bash scripts/run-crash-tests.sh`
- `bash scripts/verify-all.sh`

`bash scripts/verify-all.sh` 안에서 추가 확인된 항목:
- root / example crate `cargo fmt --check`
- `src/`에 `extern crate alloc` / `alloc::` 잔존 여부 검사
- 전체 unit/integration/std/example/embedded smoke build
- Linux host example가 `src/main.rs` 구조에서 기본 `cargo run`으로 실행되는지 확인

## 6. upstream 비교 메모

이번 slice는 core 알고리즘이 아니라 Rust 예제 crate 배치 정리다.
upstream FlashDB 자체에는 Rust `examples/` crate 구조라는 개념이 없으므로 직접 대응되는 C 소스 변경 포인트는 없다.

실제 참고 축:
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
- `~/Desktop/FlashDB/src/fdb_file.c`

비교 요약:
- 공통점
  - 예제/검증 경로는 여전히 KV/TSDB reboot/recovery smoke 검증을 빠르게 재실행하는 용도로 유지된다.
- 차이점
  - 현재 Rust 저장소는 플랫폼별 example crate를 별도로 두고 있고, 이번에는 그 단일 바이너리 예제를 `src/bin/flashdb.rs`보다 단순한 `src/main.rs`로 정리했다.
  - 즉, upstream과의 차이는 알고리즘이 아니라 Rust 패키징/개발자 UX 레이어에 있다.

## 7. 남은 차이점 / 후속 작업

plan 문서 기준으로는 07.5까지 완료 상태다.
즉시 필수인 미완료 구현 항목은 없다.

후속으로 고려할 수 있는 작업:
1. example crate 공통 코드가 더 늘어나면 `examples/common` 또는 문서화된 패턴으로 중복 정리 검토
2. 향후 새로운 crash/recovery bug가 생기면 `docs/regression-test-catalog.md`를 즉시 갱신
3. upstream parity를 더 좁히는 최적화/세부 호환성 작업이 생기면 새 plan 문서로 분리

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. 지금처럼 example UX와 검증 명령을 함께 유지하면서 새 기능/호환성 gap이 생기면 별도 계획 문서로 정의
2. 새 bugfix나 recovery slice가 생기면 테스트 추가와 함께 regression catalog 및 example 실행 문구를 동기화

## 9. 다음 세션 시작용 한 줄 요약

- "examples/linux, stm32f401re, nrf5340의 엔트리포인트를 `src/bin/flashdb.rs`에서 `src/main.rs`로 정리했고, verify-all 및 관련 문서의 예제 실행 명령도 `--bin flashdb` 없이 동기화했다. plan 07과 07.5 완료 상태는 그대로 유지된다."