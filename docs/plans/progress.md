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

## 2. 이번 작업: regression catalog 문서화 및 plan 07 마무리

이번 작업의 목표는 `docs/plans/07-testing-validation-and-rust-integration.md`의 마지막 남은 문서화 항목인 regression catalog를 실제 저장소 문서로 고정하는 것이었다.
이번 slice에서는 mock/file/Linux host validation 레이어 차이, 실행 순서, 대표 테스트 파일, crash/reboot 시나리오 위치를 별도 카탈로그 문서로 정리하고 기존 Linux validation 문서와 상호참조를 연결했다.

### 2.1 구현한 범위
- `docs/regression-test-catalog.md` 신규 추가
  - 권장 실행 순서
  - Layer 1 foundation / Layer 2 mock integration / Layer 3 std file-backed simulation / Layer 4 subprocess crash simulation 구분
  - 각 레이어별 대표 테스트 파일과 목적 정리
  - 변경 유형별 추천 검증 매트릭스 정리
  - 문제 발생 시 우선 확인 파일 정리
- `docs/linux-validation-procedure.md` 업데이트
  - 현재 실제로 커버되는 TS partial-payload, corrupted-index, sector-header corruption 시나리오 반영
  - regression catalog 문서 링크 추가

### 2.2 이번 slice의 해석
이제 plan 07의 완료 기준이 모두 충족된다.
- foundation / mock integration / std feature / crash simulation / Linux host example 검증 경로가 모두 존재한다.
- Linux host canonical procedure가 문서화돼 있다.
- regression test catalog가 문서화돼 있어 다음 세션에서 “어떤 변경에 어떤 검증을 먼저 돌릴지”를 빠르게 판단할 수 있다.

즉, plan 07은 더 이상 “crash/reboot simulation slice 진행 중”이 아니라 문서화까지 포함해 완료 상태로 본다.

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
- TSDB partial payload / corrupted index / sector-header corruption reboot recovery 유지

## 4. 이번에 수정된 파일

### 문서
- `docs/regression-test-catalog.md`
- `docs/linux-validation-procedure.md`
- `docs/plans/progress.md`

## 5. 검증 결과

이번 작업은 문서화 slice였으므로 동작 변경 없이 문서 일관성과 저장소 검증 파이프라인을 확인하는 방향으로 검증했다.

통과한 검증:
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`
- `bash scripts/run-crash-tests.sh`
- `bash scripts/verify-all.sh`

`bash scripts/verify-all.sh` 안에서 추가 확인된 항목:
- root / example crate `cargo fmt --check`
- `src/`에 `extern crate alloc` / `alloc::` 잔존 여부 검사
- 전체 unit/integration/std/example/embedded smoke build

## 6. upstream 비교 메모

이번 slice는 문서화 작업이라 core 알고리즘 자체를 바꾸지는 않았다.
다만 문서 구조는 upstream FlashDB의 테스트 자산을 참조하는 현재 Rust 프로젝트의 실제 검증 구조를 더 명확히 반영하도록 정리했다.

실제 참고 축:
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
- `~/Desktop/FlashDB/src/fdb_file.c`

비교 요약:
- 공통점
  - recovery/corruption/reboot 시나리오를 테스트로 고정해야 한다는 철학은 upstream과 같다.
- 차이점
  - 현재 Rust 쪽은 mock flash, std file-backed reopen, subprocess crash harness, Linux host example을 문서상 별도 레이어로 명시해 회귀 실행 순서를 더 운영 친화적으로 정리했다.

## 7. 남은 차이점 / 후속 작업

plan 문서 기준으로는 07.5까지 완료 상태다.
즉시 필수인 미완료 구현 항목은 없다.

후속으로 고려할 수 있는 작업:
1. `docs/flashdb-onflash-format.md` 같은 포맷 설명 문서 보강
2. 향후 새로운 crash/recovery bug가 생기면 `docs/regression-test-catalog.md`를 즉시 갱신
3. upstream parity를 더 좁히는 최적화/세부 호환성 작업이 생기면 새 plan 문서로 분리

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. 당장 새 기능을 시작하기보다 현재 검증 체계를 유지하면서 필요한 기능/호환성 gap을 별도 계획 문서로 정의
2. 새 bugfix나 recovery slice가 생기면 테스트 추가와 함께 regression catalog 동기화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07의 마지막 남은 문서화 slice로 regression test catalog를 추가했고, Linux validation 문서와 연결해 mock/file/Linux host 레이어별 검증 경로를 정리했다. 현재 docs/plans 기준으로는 plan 07과 07.5가 모두 완료 상태다."