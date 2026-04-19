# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
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
  - plan 07: 1차 crash/reboot simulation slice 완료
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- plan 07용 subprocess 기반 crash harness가 추가되어, 같은 backing file을 두고 “다른 프로세스가 중간 상태를 남기고 종료한 뒤 새 프로세스가 mount/recovery”하는 시나리오를 테스트할 수 있다.

## 2. 이번 작업: plan 07 첫 번째 file-backed crash simulation slice

이번 작업의 목표는 plan 07의 Layer 3 / Phase 7 방향에 맞춰, 단순 `reopen()` 수준을 넘어서 실제 프로세스 경계를 가진 reboot/crash regression을 추가하는 것이었다.
이번 slice에서 완료한 범위는 다음과 같다.

### 2.1 subprocess 기반 crash harness 추가
새 바이너리를 추가했다.
- `src/bin/flashdb-crash-harness.rs`

이 바이너리는 std feature 환경에서 임시 file-backed flash를 열고, 명령별로 다음 단계를 수행한다.
- 정상 KV 상태 초기화
- PRE_WRITE 상태의 tail record 주입
- CRC mismatch tail record 주입
- 새 프로세스에서 mount/recovery 후 기존 정상 값 검증
- recovery 이후 새 write가 계속 가능한지 검증

즉, 이제 test 내부 같은 프로세스에서 `reopen()`만 하는 것이 아니라,
서로 다른 프로세스가 같은 backing file을 순차적으로 열어 recovery semantics를 검증할 수 있다.

### 2.2 file-backed crash regression test 추가
새 std-feature 테스트를 추가했다.
- `tests/crash_scenarios.rs`

검증하는 것:
1. `kv_process_restart_recovers_from_pre_write_tail`
   - 첫 프로세스가 정상 KV를 기록
   - 다음 프로세스가 PRE_WRITE tail을 남김
   - 세 번째 프로세스가 mount 시 broken tail을 버리고 기존 stable 값을 유지하는지 확인
2. `kv_process_restart_recovers_from_crc_mismatched_tail`
   - 첫 프로세스가 정상 KV를 기록
   - 다음 프로세스가 CRC mismatch tail을 남김
   - 세 번째 프로세스가 mount 시 bad tail을 버리고 기존 good 값을 유지하는지 확인

두 테스트 모두 recovery 이후 새 KV write까지 성공해야 통과한다.

### 2.3 crash test 실행 스크립트 추가
- `scripts/run-crash-tests.sh`

현재는 아래를 수행한다.
- `cargo test --features std --test crash_scenarios`

즉, plan 07의 file-backed crash layer를 별도로 빠르게 재실행할 수 있다.

## 3. 기존 완료 상태 유지

이전 세션까지 완료된 plan 07.5 결과는 그대로 유지된다.
- core `src/`에서 `extern crate alloc` 제거
- `src/kv/*`, `src/tsdb/*`의 `alloc::vec`, `alloc::string` 제거
- 동적 할당 대신 `heapless` 기반 bounded container 사용
- `src/config.rs`의 bounded no_alloc cap 검증 유지
- allocator 없는 embedded smoke example 유지
- `src/storage/file_sim.rs`의 std-only file-backed backend 유지
- `examples/linux/src/bin/flashdb.rs`의 file-backed smoke example 유지

즉, 현재 구조는
- core: no_std + bounded no_alloc
- host validation: std-only file-backed simulator + subprocess crash harness
로 정리되어 있다.

## 4. 이번에 수정된 파일

### 코드
- `src/bin/flashdb-crash-harness.rs`

### 테스트
- `tests/crash_scenarios.rs`

### 스크립트
- `scripts/run-crash-tests.sh`

### 문서
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업에서 통과한 검증:
- `cargo test --features std --test crash_scenarios`
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `bash scripts/run-crash-tests.sh`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`
- `bash scripts/verify-all.sh`

TDD 확인:
- 먼저 `tests/crash_scenarios.rs`를 추가했다.
- 초기 실행에서 `CARGO_BIN_EXE_flashdb-crash-harness` 경로가 없어 실패하는 것을 확인했다.
- 이후 harness 바이너리를 구현하고 테스트를 다시 실행해 통과시켰다.

## 6. upstream 비교 메모

실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
  - reboot 뒤 KV 상태를 다시 mount해서 검증하는 흐름
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
  - reboot simulation 뒤 query_count/iteration을 다시 검증하는 흐름
- `~/Desktop/FlashDB/src/fdb_file.c`
  - host/file mode를 core 밖의 파일 기반 포팅 계층으로 유지하는 방식

비교 요약:
- 공통점
  - host 환경에서 file-backed storage를 사용하고 reboot 후 mount semantics를 검증한다.
  - recovery 결과를 다시 읽기/쓰기 동작으로 확인한다.
- 차이점
  - upstream C 테스트는 라이브러리 내부 init/deinit 호출 중심의 reboot simulation이 많다.
  - 현재 Rust slice는 실제 별도 프로세스 바이너리를 도입해, backing file을 공유하는 subprocess 경계까지 검증한다.

즉, upstream의 host reboot 검증 철학은 유지하면서도 Rust 쪽은 subprocess harness를 통해 “프로세스 경계가 있는 crash/restart”를 더 직접적으로 드러내는 방향을 택했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. TSDB file-backed crash scenarios 추가
- 현재 subprocess crash regression은 KV 쪽 interrupted tail recovery에 집중되어 있다.
- 다음 slice에서는 TSDB PRE_WRITE/index/data interruption 시나리오를 같은 방식으로 추가하는 것이 좋다.

2. 더 다양한 crash injection 지점 확대
- 현재는 PRE_WRITE tail, CRC mismatch tail 두 가지를 file-backed subprocess로 검증한다.
- 이후 payload partial write, sector-header corruption, GC 중단 지점도 별도 시나리오로 늘릴 수 있다.

3. hardware smoke 절차 문서화
- STM32F302 기준 실제 flash backend smoke procedure는 아직 별도 문서로 정리되지 않았다.
- plan 07 완료 기준에 맞추려면 최소 hardware test procedure 문서가 필요하다.

4. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 두 번째 slice
   - TSDB file-backed crash/reboot scenarios 추가
   - subprocess harness에 TSDB interrupted append / recovery 검증 명령 추가
2. 그 다음 hardware validation 문서화
   - STM32F302 smoke 절차 문서 초안 작성
3. 그 다음 regression catalog 정리
   - mock/file/hardware 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07의 첫 slice 완료. std-only file-backed simulator 위에 subprocess 기반 `flashdb-crash-harness`와 `tests/crash_scenarios.rs`를 추가해서, KV PRE_WRITE/CRC tail recovery를 실제 프로세스 재시작 경계에서 검증한다. 다음은 TSDB crash scenarios와 STM32F302 hardware procedure 문서화다."