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
  - plan 07: 부분 진행
  - plan 07.5: 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 빌드된다.
- std-only file-backed simulator가 실제 `NorFlash` 백엔드로 동작하며 KV/TSDB reboot 회귀를 Linux에서 검증할 수 있다.
- Linux host example 1종(`examples/linux`)이 실제 file-backed simulator를 사용해 KV/TS smoke flow를 직접 실행한다.

## 2. 이번 작업: plan 07.5 마무리

이번 작업의 목표는 progress.md에 남아 있던 "host-side crash/file simulation 확장" 공백을 메워 plan 07.5 완료 기준을 충족시키는 것이었다.
완료한 범위는 다음과 같다.

### 2.1 core no_alloc 전환 상태 유지
이미 완료되어 있던 1차 slice의 결과는 그대로 유지된다.
- core `src/`에서 `extern crate alloc` 제거
- `src/kv/*`, `src/tsdb/*`의 `alloc::vec`, `alloc::string` 제거
- 동적 할당 대신 `heapless` 기반 bounded container 사용
- `src/config.rs`의 bounded no_alloc cap 검증 유지
- allocator 없는 embedded smoke example 유지

즉, core는 계속 no_std + bounded no_alloc 방향에 맞춰 정렬되어 있다.

### 2.2 std-only file-backed support layer 실체화
`src/storage/file_sim.rs`가 더 이상 빈 골격이 아니라 실제 재부팅 가능한 host 포팅 계층이 되었다.

핵심 변화:
- `FileFlashSimulator<const WRITE_SIZE, const ERASE_SIZE>` 추가
- backing file 자동 생성/초기화 (`0xFF` erased state 유지)
- `ReadNorFlash` / `NorFlash` 구현
- NOR 특성 유지
  - write alignment 검증
  - erase alignment 검증
  - `0 -> 1` 비트 복구 시도 시 `RequiresErase` 반환
- `reopen()` 제공으로 같은 backing file을 다시 열어 reboot 시나리오를 직접 검증 가능
- `src/storage/mod.rs`에서 `FileFlashSimulator`, `FileFlashError` 재노출

### 2.3 file-backed regression test 추가
새 std-feature 테스트를 추가했다.
- `tests/file_sim.rs`

검증하는 것:
- file-backed KVDB 상태가 reopen 뒤에도 유지되는지
- file-backed TSDB append/iterate 상태가 reopen 뒤에도 유지되는지
- simulator가 erase 없이 `0 -> 1` 비트 복구를 허용하지 않는지

이 테스트는 `cargo test --features std`에서 실행된다.

### 2.4 Linux host example을 실제 file-backed smoke로 전환
`examples/linux/src/bin/flashdb.rs`는 더 이상 `MockFlash` 기반 RAM smoke만 하지 않는다.
이제 실제 임시 파일 기반 `FileFlashSimulator`를 사용한다.

즉, host example이 다음을 직접 증명한다.
- KVDB file-backed write/reboot/read
- TSDB file-backed append/reboot/query
- core는 no_alloc로 유지되면서도 std-only host simulation은 독립 계층으로 계속 활용 가능

## 3. 이번에 수정된 파일

### 코드
- `src/storage/file_sim.rs`
- `src/storage/mod.rs`
- `examples/linux/src/bin/flashdb.rs`

### 테스트
- `tests/file_sim.rs`

### 문서
- `docs/plans/progress.md`

## 4. 검증 결과

이번 작업에서 통과한 검증:
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `cargo run --manifest-path examples/linux/Cargo.toml --bin flashdb`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`
- `bash scripts/verify-all.sh`

보조 TDD 확인:
- `cargo test --features std --test file_sim`
  - 처음에는 새 simulator API 부재로 compile failure를 확인했다.
  - 구현 후에는 file-backed persistence / erase semantics 테스트가 통과했다.

## 5. upstream 비교 메모

이번 마무리 작업은 upstream FlashDB의 host/file mode 철학을 Rust 쪽에서 더 명확히 드러내는 쪽이다.
실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/docs/porting.md`
  - core DB 위에 flash `read`/`write`/`erase` 포팅 계층을 붙이는 구조 설명
- `~/Desktop/FlashDB/src/fdb_file.c`
  - host/file mode를 core 밖의 파일 기반 포팅 계층으로 유지

비교 요약:
- 공통점
  - core logic 바깥에 std/file 기반 포팅 계층을 둔다.
  - Linux host에서 reboot 성격의 검증을 계속 가능하게 한다.
- 차이점
  - upstream C는 섹터별 파일 캐시를 두는 구조다.
  - 현재 Rust 구현은 우선 correctness와 단순성을 위해 "단일 backing file + reopen 기반" simulator를 택했다.

즉, upstream의 계층 분리 철학은 유지하면서도 Rust 쪽은 더 단순한 std-only backend로 plan 07.5 완료 기준을 충족시켰다고 보는 것이 정확하다.

## 6. 남은 차이점 / 후속 작업

plan 07.5는 완료되었지만, 이후 개선 여지는 남아 있다.

1. bounded cap 완화/재설계
- 현재는 `heapless` 기반 고정 상한을 둔 상태다.
- 더 일반적인 API로 가려면 caller-provided scratch / streaming iterator / callback scan 쪽으로 추가 리팩토링이 필요하다.

2. KV GC의 snapshot 의존 축소
- 현재 GC는 no_alloc이긴 하지만 bounded live-set snapshot을 사용한다.
- 이후 필요하면 더 upstream-like 하거나 더 streaming-friendly 한 compacting/sector-copy 전략으로 바꿀 수 있다.

3. TSDB iterator/query의 snapshot 의존 축소
- 현재 TSDB iter/query도 heapless snapshot 기반이다.
- correctness는 유지되지만, 완전한 no_alloc-friendly API 관점에서는 streaming 형태가 더 이상적이다.

4. host-side crash simulation 확장
- 현재는 file-backed reboot regression이 가능해졌다.
- 다음 단계에서는 process restart / crash injection / partial write 시나리오를 더 체계적으로 얹을 수 있다.

## 7. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/07-testing-validation-and-rust-integration.md`
   - host reboot/crash simulation 강화
   - std-only file-backed support 위에 crash regression 추가
   - hardware validation / host validation 구분 정리
2. 그 다음 07.5 후속 최적화 성격 작업
   - KV/TS iterator/query/GC에서 snapshot 의존을 더 줄이는 방향 검토
   - caller-provided buffer / streaming API 후보 설계
3. 그 다음 embedded example을 실제 hardware flash backend 쪽으로 확장

## 8. 다음 세션 시작용 한 줄 요약

- "plan 07.5 완료. core src의 no_alloc 전환은 유지되고, std-only `FileFlashSimulator`가 실제 file-backed host backend로 동작한다. Linux example도 이제 file-backed reboot smoke를 수행한다. 다음은 plan 07에서 crash/reboot validation을 더 강화하는 작업이다."