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

## 2. 이번 작업: examples 기반 사용/포팅 가이드 문서화

이번 작업의 목표는 현재 저장소의 example crate를 처음 보는 사용자가
- Linux host에서 어떻게 바로 써볼 수 있는지,
- embedded target으로 어떻게 옮겨야 하는지,
를 한 문서에서 이해할 수 있게 만드는 것이었다.

이번 slice에서는 Linux / STM32F401RE / nRF5340 example의 실제 코드 흐름을 기준으로 사용자용 문서를 새로 추가했다.

### 2.1 구현한 범위
- `docs/how-to-use-and-port.md` 신규 추가
  - Linux host quick start
  - KVDB / TSDB 최소 사용 패턴 정리
  - `FileFlashSimulator` 기반 host-side 시작 방법 정리
  - embedded example의 목적(MockFlash 기반 smoke build) 설명
  - 실제 보드 포팅 절차와 주의점 정리
- `docs/linux-validation-procedure.md` 업데이트
  - 새 사용/포팅 가이드 문서 링크 추가

### 2.2 이번 slice의 해석
이번 변경은 core 동작이 아니라 문서화/온보딩 정리다.
- 새 사용자는 Linux example 하나만 보고도 KV/TSDB 기본 흐름을 따라갈 수 있다.
- embedded 사용자는 STM32/nRF example을 출발점으로 삼아 `MockFlash`를 실제 `NorFlash` backend로 교체해야 한다는 점을 명확히 이해할 수 있다.
- validation 문서와 usage/porting 문서가 분리되어, 검증 절차와 사용자 시작 가이드의 역할이 더 선명해졌다.

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

### 사용자 문서
- `docs/how-to-use-and-port.md`
- `docs/linux-validation-procedure.md`
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업은 examples를 바탕으로 한 사용자 문서 추가 slice였고, 문서 내용이 현재 example/검증 흐름과 어긋나지 않는지 확인하는 방향으로 검증했다.

통과한 검증:
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --target thumbv8m.main-none-eabihf`
- `bash scripts/run-crash-tests.sh`
- `bash scripts/verify-all.sh`

문서 일관성 확인 포인트:
- Linux example가 실제로 `FileFlashSimulator` + reboot/reopen flow를 설명하는지 확인
- embedded example가 실제로 `MockFlash` 기반 smoke example임을 문서가 분명히 설명하는지 확인
- 사용/포팅 가이드와 validation 문서의 역할이 서로 겹치지 않고 연결되는지 확인

## 6. upstream 비교 메모

이번 slice는 구현 변경이 아니라 Rust 저장소의 example/문서 계층 정리다.
upstream FlashDB에는 Rust example crate나 `how-to-use-and-port.md` 같은 온보딩 문서 구조가 직접 대응되지는 않는다.

실제 참고 축:
- `examples/linux/src/main.rs`
- `examples/stm32f401re/src/main.rs`
- `examples/nrf5340/src/main.rs`
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`

비교 요약:
- 공통점
  - 사용자가 최소 smoke 흐름부터 시작하고, 이후 검증/회귀 시나리오로 확장해야 한다는 점은 upstream 테스트 철학과 맞닿아 있다.
- 차이점
  - 현재 Rust 저장소는 host example, embedded example, validation 문서, usage/porting 문서를 분리해 개발자 온보딩을 더 명시적으로 제공한다.
  - 즉, upstream과의 차이는 주로 문서화/패키징/온보딩 레이어에 있다.

## 7. 남은 차이점 / 후속 작업

plan 문서 기준으로는 07.5까지 완료 상태다.
즉시 필수인 미완료 구현 항목은 없다.

후속으로 고려할 수 있는 작업:
1. 실제 하드웨어 flash backend 예제가 추가되면 `docs/how-to-use-and-port.md`에 hardware-specific 섹션 확장
2. 향후 새로운 crash/recovery bug가 생기면 `docs/regression-test-catalog.md`를 즉시 갱신
3. upstream parity를 더 좁히는 최적화/세부 호환성 작업이 생기면 새 plan 문서로 분리

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. 현재 usage/porting 문서를 기준으로 실제 보드 포팅 시도 후, 모호했던 부분을 문서에 다시 반영
2. 새 bugfix나 recovery slice가 생기면 테스트 추가와 함께 regression catalog와 usage 문서를 함께 동기화

## 9. 다음 세션 시작용 한 줄 요약

- "examples/linux/stm32f401re/nrf5340 코드를 기준으로 `docs/how-to-use-and-port.md`를 추가해 Linux quick start, KV/TSDB 최소 사용 패턴, 그리고 embedded `MockFlash` 예제를 실제 `NorFlash` backend로 바꾸는 포팅 절차를 문서화했다."