# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 진행 기준: `docs/plans/07.5-no-std-no-alloc-transition.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 완료
  - plan 06: 완료
  - plan 07: 부분 진행
  - plan 07.5: 1차 구현 slice 완료

현재 프로젝트는 다음 상태다.
- KVDB: MVP + recovery/GC/iterator/integrity까지 유지된다.
- TSDB: variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 유지된다.
- core `src/`는 `extern crate alloc` 및 `alloc::` 의존 없이 동작한다.
- Linux host 테스트와 std feature 테스트는 계속 유지된다.
- embedded example 2종(stm32f401re, nrf5340)은 allocator 없이 다시 빌드된다.

## 2. 이번 작업: plan 07.5 1차 no_alloc 전환

이번 작업의 목표는 plan 07.5를 문서 상태에서 실제 코드 리팩토링 단계로 옮기는 것이었다.
완료한 범위는 다음과 같다.

### 2.1 core `alloc` 제거
- `src/lib.rs`의 `extern crate alloc` 제거
- `src/kv/*`, `src/tsdb/*`의 `alloc::vec`, `alloc::string` 제거
- 동적 할당 대신 `heapless` 기반 bounded container로 치환

핵심 변화:
- `KvOwnedRecord`
  - `String` -> `heapless::String<MAX_KV_KEY_LEN>`
  - `Vec<u8>` -> `heapless::Vec<u8, MAX_KV_VALUE_LEN>`
- `TsOwnedRecord`
  - payload -> `heapless::Vec<u8, MAX_TS_PAYLOAD_LEN>`
- TSDB sector runtime table
  - heap `Vec` -> `heapless::Vec<TsSectorRuntime, MAX_TS_SECTORS>`

즉, 현재 core는 heap allocator 없이도 동작하지만,
완전한 무제한 동적 크기 대신 "bounded no_alloc" 방식으로 정리되었다.

### 2.2 no_alloc 경계용 bounded runtime cap 도입
`src/config.rs`에 다음 bounded cap을 추가하고 validation에 연결했다.
- `MAX_KV_KEY_LEN`
- `MAX_KV_VALUE_LEN`
- `MAX_KV_RECORDS`
- `MAX_TS_PAYLOAD_LEN`
- `MAX_TS_RECORDS`
- `MAX_TS_SECTORS`
- `MAX_RUNTIME_WRITE_SIZE`
- `MAX_TS_HEADER_LEN`
- `MAX_TS_INDEX_LEN`

검증되는 것:
- KV key/value 길이가 no_alloc bounded cap을 넘으면 reject
- TS fixed blob 길이가 bounded payload cap을 넘으면 reject
- region write size / sector count가 bounded runtime cap을 넘으면 reject
- KV 신규 live key 수가 bounded snapshot cap을 넘기기 전에 reject
- TS variable payload가 bounded payload cap을 넘으면 reject
- TS append 수가 bounded snapshot cap을 넘기기 전에 reject

이 방식은 plan 07.5의 "bounded memory 우선" 원칙에는 맞고,
향후 caller-provided scratch / streaming API로 더 일반화할 여지는 남겨 둔다.

### 2.3 TSDB/KV iterator와 snapshot의 no_alloc 정렬
완전한 streaming iterator로 아직 바꾸지는 않았지만,
현재 snapshot/iterator 경로는 더 이상 heap allocation에 의존하지 않는다.

구체적으로:
- KV iterator snapshot은 bounded `heapless::Vec`로 유지
- KV GC live-set snapshot도 같은 bounded container를 재사용
- TSDB record snapshot도 bounded `heapless::Vec` 기반으로 유지
- 테스트는 새 bounded record 타입에 맞게 `.as_slice()` 비교로 조정

즉, 이번 slice는 "heap 제거"가 목적이고,
다음 slice에서 필요하면 iterator/query/GC를 caller-provided buffer나 streaming scan 쪽으로 더 밀어낼 수 있다.

### 2.4 embedded example allocator 제거
다음 두 embedded smoke example에서 allocator 초기화와 `embedded-alloc` 의존을 제거했다.
- `examples/stm32f401re`
- `examples/nrf5340`

결과:
- 두 example 모두 allocator 없는 상태에서 빌드 통과
- smoke 동작은 그대로 유지
  - KV mount/format/set/get
  - TS mount/format/append/reverse check

### 2.5 host simulation / std-only support 유지
`src/storage/file_sim.rs`는 그대로 std-only support layer로 남겨 두었다.
이 방향은 upstream FlashDB의 porting guide와 file mode 철학과 맞춘 것이다.

실제 참고한 upstream 근거:
- `~/Desktop/FlashDB/docs/porting.md`
  - core DB 위에 flash `read`/`write`/`erase` 포팅 계층을 붙이는 구조를 설명
- `~/Desktop/FlashDB/src/fdb_file.c`
  - host/file mode를 core logic 바깥의 파일 기반 포팅 계층으로 유지

현재 Rust 쪽도 같은 방향으로,
- core는 allocator 없는 bounded no_std 쪽으로 정렬하고
- host/file simulation은 std-only support로 남기는 구조를 유지한다.

## 3. 이번에 수정된 파일

### 코드
- `Cargo.toml`
- `src/lib.rs`
- `src/config.rs`
- `src/kv/db.rs`
- `src/kv/iter.rs`
- `src/tsdb/iter.rs`
- `src/tsdb/db.rs`

### 테스트
- `tests/config_validation.rs`
- `tests/no_alloc_bounds.rs`
- `tests/kv_plan05.rs`
- `tests/ts_basic.rs`
- `tests/ts_rollover.rs`

### examples
- `examples/stm32f401re/Cargo.toml`
- `examples/stm32f401re/src/bin/flashdb.rs`
- `examples/nrf5340/Cargo.toml`
- `examples/nrf5340/src/bin/flashdb.rs`

### 검증 스크립트 / 문서
- `scripts/verify-all.sh`
- `docs/plans/progress.md`

## 4. 검증 결과

이번 작업에서 통과한 검증:
- `cargo fmt`
- `cargo test`
- `cargo test --features std`
- `cargo test --test no_alloc_bounds`
- `cargo build --manifest-path examples/stm32f401re/Cargo.toml --bin flashdb --target thumbv7em-none-eabihf`
- `cargo build --manifest-path examples/nrf5340/Cargo.toml --bin flashdb --target thumbv8m.main-none-eabihf`
- `bash scripts/verify-all.sh`

또한 `scripts/verify-all.sh`는 이제 `src/` 아래에 `extern crate alloc` / `alloc::`가 남아 있으면 실패하도록 점검한다.

## 5. upstream 비교 메모

이번 리팩토링은 upstream 동작 의미를 바꾸려는 작업이 아니었다.
의도는 semantics를 유지한 채 메모리 모델을 바꾸는 것이었다.

비교 요약:
- KV/TSDB의 기존 동작 semantics는 그대로 유지했다.
- host simulation을 core 밖의 std-only support로 두는 방향은 upstream `docs/porting.md` / `src/fdb_file.c`의 porting/file mode 철학과 일치한다.
- 다만 현재 Rust 구현은 upstream C처럼 런타임 크기 자유도를 그대로 가져가기보다,
  bounded no_alloc cap을 먼저 두는 Rust-first 방식으로 정리했다.

즉, 이번 slice는 "upstream 포팅 계층 분리 철학 유지 + Rust no_alloc bounded memory 적용"으로 보는 것이 정확하다.

## 6. 남은 차이점 / 후속 작업

plan 07.5가 완전히 끝난 것은 아니다. 현재 남은 핵심 항목은 다음과 같다.

1. bounded cap 완화/재설계
- 현재는 `heapless` 기반 고정 상한을 둔 상태다.
- 더 일반적인 API로 가려면 caller-provided scratch / streaming iterator / callback scan 쪽으로 추가 리팩토링이 필요하다.

2. KV GC의 snapshot 의존 축소
- 현재 GC는 no_alloc이긴 하지만 bounded live-set snapshot을 사용한다.
- 이후 필요하면 더 upstream-like 하거나 더 streaming-friendly 한 compacting/sector-copy 전략으로 바꿀 수 있다.

3. TSDB iterator/query의 snapshot 의존 축소
- 현재 TSDB iter/query도 heapless snapshot 기반이다.
- correctness는 유지되지만, 완전한 no_alloc-friendly API 관점에서는 streaming 형태가 더 이상적이다.

4. host-side crash/file simulation 확장
- `file_sim.rs`는 아직 얇은 골격이다.
- plan 07 / 07.5 후속으로 reboot/crash regression을 더 체계적으로 얹을 수 있다.

## 7. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. plan 07.5 두 번째 slice
   - KV/TS iterator/query/GC에서 snapshot 의존을 더 줄이는 방향 검토
   - caller-provided buffer / streaming API 후보 설계
2. 그 다음 `docs/plans/07-testing-validation-and-rust-integration.md`
   - host reboot/crash simulation 강화
   - std-only file-backed support 실체화
3. 그 다음 embedded example을 실제 hardware flash backend 쪽으로 확장

## 8. 다음 세션 시작용 한 줄 요약

- "plan 07.5 1차 구현 완료. core src에서 alloc 의존을 제거했고 heapless 기반 bounded no_alloc 구조로 KV/TSDB를 재정렬했다. Linux test/std test는 유지되고 stm32f401re/nrf5340 example도 allocator 없이 빌드된다. 다음은 snapshot 의존을 더 줄이는 07.5 후속 slice다."