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
  - plan 07: 4차 crash/reboot simulation slice 완료
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- subprocess 기반 `flashdb-crash-harness`가 KV crash recovery, KV sector-header corruption recovery, TSDB reboot/query, TSDB PRE_WRITE tail recovery, TSDB status mutation reboot, TSDB deleted-status reboot, TSDB clean/reset reboot까지 검증한다.

## 2. 이번 작업: plan 07 네 번째 corruption/reboot slice

이번 작업의 목표는 plan 07의 file-backed subprocess 레이어를 더 공격적인 corruption/reboot 경로까지 확장하는 것이었다.
이번 slice에서 완료한 범위는 다음과 같다.

### 2.1 TSDB deleted-status reboot persistence 추가
`tests/crash_scenarios.rs`와 `src/bin/flashdb-crash-harness.rs`를 확장해서 deleted-status reboot 검증을 추가했다.

추가된 흐름:
- `ts-init-delete-window`
  - file-backed flash에 TS records 3개 기록
- `ts-set-deleted-and-reboot-check`
  - 새 프로세스에서 mount
  - timestamp 20 record를 `TSL_DELETED`로 변경
  - 같은 backing file을 다시 mount
  - `iter_by_time(10, 30)` 상태 목록 검증
  - `query_count(..., TSL_DELETED)` / `query_count(..., TSL_WRITE)` 검증

즉, 이제 TSDB deleted-status transition도 reboot 이후 유지되는지 subprocess 경계에서 확인한다.

### 2.2 TSDB set_status의 multi-step transition 보정
이번 slice에서 실제 버그가 드러났고 함께 수정했다.

문제:
- 기존 `src/tsdb/db.rs`의 `set_status(...)`는 target status 하나만 바로 program 했다.
- 그런데 status table 특성상 `TSL_WRITE -> TSL_DELETED`처럼 중간 상태를 건너뛰면 reboot 뒤 `InvalidState`가 날 수 있었다.

수정:
- `src/tsdb/db.rs`
  - `set_status(...)`가 `current_status + 1 ..= target_status`를 순차적으로 program 하도록 변경

효과:
- `TSL_USER_STATUS1`뿐 아니라 `TSL_DELETED`도 reboot 후 정상 decode/query 가능

이 구현은 upstream status transition 의도와 맞고, 현재 Rust 구조에서도 truthful한 recovery 보정이다.

### 2.3 KV corrupted next-sector header file-backed recovery 추가
기존 mock test에만 있던 corrupted next-sector header recovery를 subprocess file-backed 시나리오로 옮겼다.

추가된 흐름:
- `kv-init-two-sector-fill`
  - 2-sector KV region에서 first sector를 채우는 write 수행
- `kv-corrupt-next-sector-header`
  - second sector header magic을 file-backed flash에서 직접 손상
- `kv-check-corrupted-sector-recovery`
  - 새 프로세스에서 mount
  - 기존 live value가 유지되는지 확인
  - recovery 뒤 새 key write가 가능한지 확인

즉, KVDB도 단순 tail recovery뿐 아니라 sector header corruption 복구를 subprocess reboot 기준으로 검증한다.

### 2.4 crash_scenarios 레이어 확대
`tests/crash_scenarios.rs`는 이제 다음 subprocess 시나리오를 포함한다.
- KV PRE_WRITE tail recovery
- KV CRC mismatch tail recovery
- KV corrupted next-sector header recovery
- TSDB PRE_WRITE tail recovery
- TSDB reboot 후 reverse/query/range semantics 유지
- TSDB status mutation 후 reboot
- TSDB deleted-status reboot
- TSDB clean/reset 후 reboot

즉, plan 07의 Layer 3 file-backed reboot simulation은 단순 reopen smoke를 넘어, 실제 corruption/recovery regression 카탈로그의 핵심 골격을 갖추기 시작했다.

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
- TSDB PRE_WRITE tail / reboot query / reboot append recovery 유지
- TSDB status mutation reboot / clean reboot 유지

즉, 현재 구조는
- core: no_std + bounded no_alloc
- host validation: std-only file-backed simulator + subprocess crash harness
로 정리되어 있다.

## 4. 이번에 수정된 파일

### 코드
- `src/bin/flashdb-crash-harness.rs`
- `src/tsdb/db.rs`

### 테스트
- `tests/crash_scenarios.rs`

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
- 먼저 `tests/crash_scenarios.rs`에 deleted-status reboot / KV sector-header corruption reboot 테스트를 추가했다.
- 초기 실행에서 harness에 새 `kv-*` / `ts-*` 명령이 없어 실패하는 것을 확인했다.
- 이후 harness 명령을 구현하고 재실행했다.
- deleted-status reboot 경로는 처음에 reboot 후 `InvalidState`를 재현했고, 이걸 계기로 `src/tsdb/db.rs`의 status transition programming을 순차 단계 방식으로 수정해 통과시켰다.

## 6. upstream 비교 메모

실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
  - `test_fdb_tsl_set_status`
  - `test_fdb_tsl_clean`
  - reboot 뒤 `query_count`, `iter_by_time` 등을 다시 검증하는 흐름
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
  - corrupted sector/reboot 이후에도 live record와 후속 write가 유지되는 recovery 패턴
- `~/Desktop/FlashDB/src/fdb_file.c`
  - host/file mode를 core 밖의 파일 기반 포팅 계층으로 유지하는 방식

비교 요약:
- 공통점
  - status mutation, clean/reset, reboot 뒤 query/iter semantics를 확인한다.
  - host/file 기반 storage를 사용해 persistence와 corruption recovery를 검증한다.
- 차이점
  - upstream C 테스트는 주로 같은 테스트 프로세스 안에서 init/deinit reboot simulation을 반복한다.
  - 현재 Rust slice는 subprocess harness를 통해 file-backed 상태를 다른 프로세스가 다시 여는 방식으로 검증한다.
  - 또한 TS status transition은 Rust 쪽에서 sequential flash programming을 명시적으로 수행해 reboot decode correctness를 확보했다.

즉, upstream의 host reboot 검증 철학은 유지하면서도 Rust 쪽은 subprocess 경계를 드러내는 pragmatic regression harness와 status-transition 보정으로 현재 구조에 맞는 correctness-first 구현을 택했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. payload partial write / index corruption / sector-header corruption 추가 확대
- 현재 subprocess crash regression은
  - KV PRE_WRITE tail
  - KV CRC mismatch tail
  - KV corrupted next-sector header
  - TSDB PRE_WRITE tail
  - TSDB reboot 후 query/iteration
  - TSDB status mutation reboot
  - TSDB deleted-status reboot
  - TSDB clean/reset reboot
  까지 커버한다.
- 이후 TS payload partial write, TS index corruption, TS sector-header corruption 같은 시나리오를 늘릴 수 있다.

2. hardware smoke 절차 문서화
- STM32F302 기준 실제 flash backend smoke procedure는 아직 별도 문서로 정리되지 않았다.
- plan 07 완료 기준에 맞추려면 최소 hardware test procedure 문서가 필요하다.

3. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 다섯 번째 slice
   - TS payload partial write / TS index or sector-header corruption 같은 추가 corruption scenarios 확장
2. 그 다음 hardware validation 문서화
   - STM32F302 smoke 절차 문서 초안 작성
3. 그 다음 regression catalog 정리
   - mock/file/hardware 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07의 네 번째 slice 완료. subprocess 기반 `flashdb-crash-harness`가 이제 KV corrupted sector-header recovery와 TSDB deleted-status reboot까지 검증한다. 또한 `src/tsdb/db.rs`는 deleted 같은 상위 status도 reboot 뒤 정상 decode되도록 sequential transition programming으로 수정됐다. 다음은 TS payload/index corruption과 hardware procedure 문서화다."