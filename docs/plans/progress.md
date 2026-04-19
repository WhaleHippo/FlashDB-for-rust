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

## 2. 이번 작업: plan 07 검증 방향을 Linux host 기준으로 전환

이번 작업의 목표는 plan 07 문서 집합에서 STM32F302 hardware smoke 방향을 폐기하고, 이미 강해진 Linux file-backed 검증 레이어를 canonical validation flow로 승격하는 것이었다.
이번 작업에서 완료한 범위는 다음과 같다.

### 2.1 plan 문서의 validation 목표 재정렬
다음 문서를 Linux host 중심으로 수정했다.
- `docs/plans/07-testing-validation-and-rust-integration.md`
- `docs/plans/00-top-down-roadmap.md`
- `docs/plans/README.md`

핵심 변경:
- plan 07의 목표에서 hardware smoke 중심 표현을 제거
- Layer 4를 `Linux host validation`으로 재정의
- Phase 10을 `Linux host smoke/validation procedure`로 교체
- 완료 기준도 `Linux host persistence/recovery validation 절차 문서화`로 변경

### 2.2 Linux validation procedure 문서 신설
새 문서 `docs/linux-validation-procedure.md`를 추가했다.

이 문서는 다음을 canonical flow로 정의한다.
- `cargo test`
- `cargo test --features std`
- `cargo test --features std --test crash_scenarios`
- `bash scripts/run-crash-tests.sh`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `bash scripts/verify-all.sh`

즉, 이제 이 프로젝트의 기본 검증 축은 실제 보드 smoke가 아니라 Linux host에서 반복 가능한 persistence/recovery 검증이다.

### 2.3 progress snapshot도 새 방향에 맞게 정리
`docs/plans/progress.md`의 남은 작업 우선순위와 다음 세션 요약을 Linux host 기준으로 갱신했다.

정리 결과:
- Linux validation procedure 문서화는 완료됨
- 남은 plan 07 핵심 작업은 추가 corruption scenarios와 regression catalog 문서화다

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
- 이번 작업에서는 코드 변경 없음

### 테스트
- 이번 작업에서는 테스트 변경 없음

### 문서
- `docs/plans/00-top-down-roadmap.md`
- `docs/plans/README.md`
- `docs/plans/07-testing-validation-and-rust-integration.md`
- `docs/plans/progress.md`
- `docs/linux-validation-procedure.md`

## 5. 검증 결과

이번 작업은 문서 방향 전환 작업이라 코드/테스트 재실행 대신 문서 간 정합성을 맞추는 데 집중했다.
확인한 내용:
- `docs/plans/README.md`의 권장 실행 순서와 plan 07 설명이 Linux host validation 기준으로 정렬됨
- `docs/plans/00-top-down-roadmap.md`의 상위 단계 설명도 같은 방향으로 정렬됨
- `docs/plans/07-testing-validation-and-rust-integration.md`의 Layer/Phase/완료 기준이 Linux validation 기준으로 변경됨
- `docs/linux-validation-procedure.md`가 canonical Linux 검증 절차를 제공함

문서 정합성 확인:
- `docs/plans/README.md`, `docs/plans/00-top-down-roadmap.md`, `docs/plans/07-testing-validation-and-rust-integration.md`가 모두 Linux host validation 기준으로 같은 방향을 가리키도록 맞췄다.
- 새 `docs/linux-validation-procedure.md`가 progress.md의 다음 작업에서 요구하던 Linux validation procedure 역할을 실제로 채운다.

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

2. regression catalog 문서화
- 어떤 버그/시나리오가 어떤 테스트 파일에 묶여 있는지 별도 카탈로그 문서가 있으면 다음 세션 연속성이 더 좋아진다.
- 이 카탈로그는 mock/file/Linux host validation 레이어 차이와 실행 순서를 함께 정리해야 한다.

## 8. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07 다섯 번째 slice
   - TS payload partial write / TS index or sector-header corruption 같은 추가 corruption scenarios 확장
2. 그 다음 regression catalog 정리
   - mock/file/Linux host validation 검증 레이어 차이와 실행 순서 문서화

## 9. 다음 세션 시작용 한 줄 요약

- "plan 07의 crash/reboot 검증 축은 Linux host 기준으로 재정렬됐다. STM32F302 hardware smoke 방향은 폐기했고, `docs/linux-validation-procedure.md`가 canonical 검증 절차를 제공한다. 다음은 TS payload/index corruption 확장과 regression catalog 문서화다."