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
  - plan 07: 2차 crash/reboot simulation slice 완료
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- subprocess 기반 `flashdb-crash-harness`가 KV뿐 아니라 TSDB reboot/query/crash recovery 시나리오도 검증한다.

## 2. 이번 작업: plan 07 두 번째 TSDB crash/reboot slice

이번 작업의 목표는 앞선 KV subprocess crash harness를 TSDB 쪽으로 확장해, plan 07의 Layer 3 / Phase 7 범위를 실제로 넓히는 것이었다.
이번 slice에서 완료한 범위는 다음과 같다.

### 2.1 TSDB subprocess reboot/query 시나리오 추가
`src/bin/flashdb-crash-harness.rs`를 확장해서 TSDB용 명령을 추가했다.

추가된 흐름:
- `ts-init-window`
  - file-backed flash에 TSDB를 format 후 seed records append
- `ts-check-window-query`
  - 새 프로세스에서 mount
  - reverse iteration 검증
  - `query_count` 검증
  - `iter_by_time` 검증
  - recovery 뒤 append가 계속 가능한지 검증

즉, 이제 TSDB도 실제 프로세스 경계를 넘는 reboot 이후에 query/iteration semantics가 유지되는지 검증한다.

### 2.2 TSDB interrupted PRE_WRITE tail recovery 시나리오 추가
동일 harness에 TSDB interrupted append recovery 명령을 추가했다.

추가된 흐름:
- `ts-init-seed`
  - 정상 TS records 2개 기록
- `ts-inject-prewrite-tail`
  - 다음 append slot에 `TSL_PRE_WRITE` 상태의 raw index tail 주입
- `ts-check-seed-and-append-fresh`
  - 새 프로세스에서 mount
  - PRE_WRITE tail이 live record에 섞이지 않는지 확인
  - 기존 정상 records가 유지되는지 확인
  - recovery 이후 새 append가 가능한지 확인

이로써 TSDB도 KV와 마찬가지로 “interrupted tail + reboot recovery + continued write” 경로를 file-backed subprocess 기준으로 검증한다.

### 2.3 TSDB PRE_WRITE tail 후속 append 지원 버그 수정
이번 slice에서 실제 버그가 드러났고 함께 수정했다.

문제:
- mount scan은 `TSL_PRE_WRITE` tail을 보면 멈췄지만,
- 그 뒤의 free cursor와 later scans가 PRE_WRITE slot 뒤를 제대로 건너뛰지 못해서,
- recovery 후 새 append/query 경로가 tail 뒤의 새 record를 놓치거나 append가 막힐 수 있었다.

수정:
- `src/tsdb/db.rs`
  - mount-time `scan_all_sectors(...)`
  - runtime `collect_sector_records(...)`
  - lookup `find_index_offset_for_timestamp(...)`
  에서 `TSL_PRE_WRITE` entry를 "중단된 dead tail"로 취급하고,
  - index slot은 건너뛰고
  - variable mode payload reservation은 보수적으로 소비한 것으로 간주한 뒤
  - 이후 record scan/append/query가 tail 뒤의 정상 record를 계속 다룰 수 있게 조정했다.

이 구현은 upstream의 reboot simulation 철학에는 맞으면서도, 현재 Rust 구조에서 correctness를 우선한 pragmatic recovery 방식이다.

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
- 먼저 `tests/crash_scenarios.rs`에 TSDB subprocess test 2개를 추가했다.
- 초기 실행에서 harness에 `ts-*` 명령이 없어 실패하는 것을 확인했다.
- harness 구현 후에는 `TSL_PRE_WRITE` tail 뒤 append/query가 깨지는 failure를 실제로 관측했다.
- 그 다음 `src/tsdb/db.rs` recovery/scan 로직을 수정해 테스트를 통과시켰다.

## 6. upstream 비교 메모

실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
  - reboot 뒤 `iter_by_time`, `query_count`, append 결과를 다시 검증하는 흐름
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
  - reboot simulation을 여러 번 반복하면서 recovery 후 write 지속 가능성을 보는 패턴
- `~/Desktop/FlashDB/src/fdb_file.c`
  - host/file mode를 core 밖의 파일 기반 포팅 계층으로 유지하는 방식

비교 요약:
- 공통점
  - host 환경에서 file-backed storage를 사용하고 reboot 후 mount semantics를 검증한다.
  - recovery 결과를 다시 iteration/query/write 동작으로 확인한다.
- 차이점
  - upstream C 테스트는 주로 같은 테스트 프로세스 안에서 init/deinit reboot simulation을 반복한다.
  - 현재 Rust slice는 별도 프로세스 바이너리와 file-backed backend를 써서, subprocess 경계가 있는 TSDB reboot/query/crash 경로를 직접 검증한다.
  - 또한 PRE_WRITE tail 뒤의 후속 append/query를 보수적으로 허용하기 위해 dead-tail slot skip 방식을 명시적으로 적용했다.

즉, upstream의 host reboot 검증 철학은 유지하면서도 Rust 쪽은 subprocess harness와 tail-skip recovery로 현재 구조에 맞는 correctness-first 구현을 택했다.

## 7. 남은 차이점 / 후속 작업

plan 07은 아직 전체 완료가 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. 더 다양한 crash injection 지점 확대
- 현재 subprocess crash regression은
  - KV PRE_WRITE tail
  - KV CRC mismatch tail
  - TSDB PRE_WRITE tail
  - TSDB reboot 후 query/iteration
  까지 커버한다.
- 이후 payload partial write, sector-header corruption, GC 중단 지점도 별도 시나리오로 늘릴 수 있다.

2. TSDB status mutation / clean 경로의 file-backed crash 검증
- 현재 TSDB subprocess 시나리오는 append/query/recovery 중심이다.
- status mutation 후 reboot, clean 후 reboot 같은 시나리오를 추가할 수 있다.

3. hardware smoke 절차 문서화
- STM32F302 기준 실제 flash backend smoke procedure는 아직 별도 문서로 정리되지 않았다.
- plan 07 완료 기준에 맞추려면 최소 hardware test procedure 문서가 필요하다.

4. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 세 번째 slice
   - payload partial write / sector-header corruption / TS status-change reboot 같은 추가 crash scenarios 확장
2. 그 다음 hardware validation 문서화
   - STM32F302 smoke 절차 문서 초안 작성
3. 그 다음 regression catalog 정리
   - mock/file/hardware 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07의 두 번째 slice 완료. `flashdb-crash-harness`가 이제 TSDB subprocess reboot/query와 PRE_WRITE tail recovery도 검증한다. 이 과정에서 `src/tsdb/db.rs`가 PRE_WRITE dead tail 뒤의 후속 append/query를 계속 처리하도록 수정됐다. 다음은 더 다양한 crash injection과 STM32F302 hardware procedure 문서화다."