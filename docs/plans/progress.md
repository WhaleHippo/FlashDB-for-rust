# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 진행 기준: `docs/plans/06-tsdb-plan.md` 완료
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 완료
  - plan 06: 완료
  - plan 07 이후: 미구현

즉, 현재 프로젝트는:
- KVDB 쪽은 MVP + recovery/GC/iterator/integrity까지 구현 및 검증 완료 상태이고,
- TSDB 쪽은 variable/fixed blob mode 기준 mount/append/query/rollover/clean/reset까지 plan 06 완료 기준을 충족한 상태다.

## 2. 이번에 plan 06에서 마무리한 것

이번 작업에서는 progress snapshot에 남아 있던 plan 06의 마지막 공백을 메웠다.
핵심은 다음 두 가지였다.

1. rollover on/off 정책 완성
2. fixed-size blob mode 구현 및 reboot 복원 검증

### 2.1 TSDB rollover 정책 완료
`src/tsdb/db.rs`를 확장해 sector full 이후의 next-sector 선택을 plan 06 완료 기준에 맞게 마무리했다.

구현된 내용:
- `TsdbConfig`에 `rollover: bool` 추가
- `rollover=false`일 때 마지막 sector까지 가득 차면 `Error::NoSpace` 반환
- `rollover=true`일 때 다음 sector를 ring처럼 순환 선택
- 순환 대상 sector가 기존 데이터를 가지고 있으면 erase 후 재사용
- wrap 이후에도
  - `oldest_sector`
  - `current_sector`
  - `last_timestamp`
  이 올바르게 유지되도록 조정
- TSDB snapshot/lookup 순회가 물리 sector 0..N 고정 순서가 아니라, `oldest_sector`부터 ring 순서로 순회하도록 수정

원본 FlashDB 비교 메모:
- upstream `src/fdb_tsdb.c`의 `update_sec_status`, `tsl_append`, sector wrap 흐름을 기준으로 구현했다.
- Rust 구현도 sector full 후 next sector를 선택하고, rollover=true일 때 앞쪽 sector를 재사용하는 핵심 의미를 맞췄다.
- 다만 upstream은 sector header의 `end_info[0/1]`를 append 경로에서 적극 활용하지만,
  현재 Rust 구현은 mount 시 index area를 다시 스캔해서 runtime state를 복원하는 보수적 방식을 유지한다.
- 즉, rollover semantics는 맞췄고, sector-header 기반 최적화는 후속 parity/optimization 항목으로 남겨두었다.

### 2.2 fixed-size blob mode 구현
기존에는 `BlobMode::Fixed(_)`가 TSDB에서 아예 거부되었는데, 이번에 실제 동작하도록 연결했다.

구현된 내용:
- `BlobMode::Fixed(len)` -> `TsBlobMode::Fixed(len as u32)` 매핑 허용
- fixed mode에서 index에는 timestamp/status만 기록하고,
  payload 위치는 sector 내부 고정 slot 계산으로 복원
- append 시 payload 길이가 fixed size와 정확히 일치해야 하도록 검증
- iter / iter_reverse / mount recovery가 fixed mode payload를 올바르게 읽도록 수정
- reboot 후에도 fixed mode record가 정상 복원되는 테스트 추가

원본 FlashDB 비교 메모:
- upstream `read_tsl`이 fixed-size 모드에서 index address로 sector 내 payload 위치를 역산하는 방식을 참고했다.
- 현재 Rust 구현도 동일한 방향으로, index에 log_addr/log_len을 저장하지 않고 slot 계산으로 payload 위치를 복원한다.
- 즉, fixed-size payload 위치 계산 방식은 upstream 구조와 본질적으로 동일하다.

### 2.3 full sector mount/recovery 보정
rollover + fixed mode를 붙이는 과정에서 sector가 index/data 경계까지 꽉 찬 경우 reboot scan이 payload 영역을 추가 index로 오해할 수 있는 경계 문제가 드러났다.

조정한 내용:
- mount scan이 단순히 sector 끝까지 index를 읽는 것이 아니라,
  현재 계산된 `empty_data_offset`과 겹치지 않는 범위까지만 index를 읽도록 수정
- 덕분에 full sector 상태에서도 reboot 후 `Decode(InvalidState)` 없이 정상 복원됨

이 부분은 upstream의 `remain`, `empty_idx`, `empty_data` 관리 의미와 맞닿아 있으며,
현재 Rust 구현에서도 동일한 dual-ended layout invariant를 더 정확히 지키도록 만든 수정이다.

## 3. plan 06 완료 판단

이제 plan 06의 완료 기준은 충족했다고 판단한다.

문서의 완료 기준:
- append 후 forward iter 정상
- reverse iter 정상
- time-range query 정상
- rollover on/off 정책 정상
- clean/reset 정상
- reboot 후 current/oldest/last_time 복원 가능

현재 상태:
- forward iteration: 완료
- reverse iteration: 완료
- iter_by_time / query_count: 완료
- status mutation: 완료
- clean/reset: 완료
- rollover=false no-space 정책: 완료
- rollover=true ring overwrite 정책: 완료
- reboot 후 current/oldest/last_timestamp 복원: 완료
- fixed-size blob mode: 구현 완료

따라서 plan 06은 더 이상 "진행 중"이 아니라 완료로 옮긴다.

## 4. 이번 slice에서 수정된 파일

### 코드
- `src/config.rs`
- `src/tsdb/db.rs`

### 테스트
- `tests/config_validation.rs`
- `tests/ts_basic.rs`
- `tests/ts_rollover.rs`

### 문서
- `docs/plans/progress.md`

## 5. 테스트로 검증된 것

이번 작업도 TDD로 진행했다.

먼저 실패를 확인한 테스트:
- `cargo test --test ts_rollover`
  - 처음에는 `TsdbConfig`에 rollover 설정이 없어서 컴파일 단계에서 실패
  - 이후 rollover/fixed-mode 동작을 붙인 뒤에도 wrap 후 reboot에서 `Decode(InvalidState)`가 발생하는 실패를 재현
  - sector full scan 경계를 수정한 뒤 통과

최종 통과한 검증:
- `cargo test --test ts_rollover`
- `cargo fmt`
- `cargo test`
- `cargo test --features std`

직접 검증된 핵심 시나리오:
- rollover=false에서 마지막 sector 이후 append가 `Error::NoSpace`로 종료됨
- rollover=true에서 oldest/current sector가 ring semantics에 맞게 갱신됨
- rollover 후 살아 있는 record만 iteration 결과에 남음
- rollover 후 reboot해도 oldest/current/last_timestamp와 live window가 유지됨
- fixed blob mode에서 append / iter / iter_reverse / reboot recovery가 정상 동작함
- fixed blob mode에서 payload 길이 불일치를 거부함
- 기존 variable-mode TSDB 테스트와 KVDB 테스트 전부 회귀 없이 유지됨

## 6. 남아 있는 차이점과 후속 작업 성격

plan 06 완료와 별개로, upstream 대비 아직 남아 있는 차이점은 있다.
다만 이것들은 현재 문서의 완료 기준을 막는 필수 공백은 아니라서 plan 07 이전의 최적화/패리티 개선 항목으로 본다.

대표 차이점:
- append 시 sector header `end_info[0/1]`를 upstream처럼 증분 갱신하지 않음
- mount 시 sector header 정보만으로 끝내지 않고 index area 재스캔으로 runtime state를 복원함
- `iter_by_time` / `query_count`가 sector-level coarse filtering + 내부 search 최적화보다 correctness-first full scan에 가까움
- `set_status(...)` public API가 upstream의 `fdb_tsl_t` 핸들 기반이 아니라 timestamp lookup 기반임

즉, semantics는 plan 06 완료 수준에 도달했고,
앞으로 남은 것은 주로 upstream parity와 성능 최적화 성격이다.

## 7. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/07-testing-validation-and-rust-integration.md`
   - host simulation
   - crash/recovery validation
   - 실제 Rust integration 예제
2. 필요하면 그 다음에 TSDB parity/optimization 정리
   - `end_info` 증분 갱신
   - range query sector filtering
   - status mutation handle API 검토

## 8. 다음 세션 시작용 한 줄 요약

- "plan 06 완료. TSDB는 variable/fixed blob mode, forward/reverse/range query, status mutation, clean/reset, rollover on/off, reboot 복원까지 구현됐다. 다음은 plan 07 검증/통합 단계다."