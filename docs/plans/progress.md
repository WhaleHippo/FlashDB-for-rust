# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 진행 중인 plan: `docs/plans/06-tsdb-plan.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 완료
  - plan 06: 진행 중
  - plan 07 이후: 미구현

즉, 현재 프로젝트는:
- KVDB 쪽은 MVP + recovery/GC/iterator/integrity까지 구현 및 검증 완료 상태이고,
- TSDB 쪽은 layout codec 기반 위에 실제 runtime/mount/append/forward-iteration의 첫 slice가 들어간 상태다.

## 2. 이번에 plan 06에서 완료한 것

이번 slice는 plan 06의 전 범위를 한 번에 끝내기보다, 실제로 바로 검증 가능한 첫 구현 단위에 맞춰 다음 범위를 완료했다.

### 2.1 TSDB runtime state / mount 기초
`src/tsdb/db.rs`를 중심으로 TSDB 실구현을 시작했다.

구현된 내용:
- `TsDb::mount(flash, config)` 추가
- `TsDb<F>`를 실제 flash backend 위에서 동작하는 generic DB 타입으로 확장
- region/write granularity 기반 `TsLayout` 런타임 계산
- sector별 runtime metadata 복원
  - `store_status`
  - `start_time`
  - `end_time`
  - `empty_index_offset`
  - `empty_data_offset`
  - `entry_count`
- mount 시 전체 sector를 스캔해
  - `current_sector`
  - `oldest_sector`
  - `last_timestamp`
  를 복원하도록 구현

원본 FlashDB 비교 메모:
- upstream `src/fdb_tsdb.c`의 `read_sector_info`, `fdb_tsdb_init` 흐름을 참고했다.
- 다만 이번 Rust slice는 sector header의 `end_info`를 append 때마다 갱신하는 방식까지는 아직 구현하지 않았고,
  mount 시 index area를 다시 스캔해서 current/oldest/last_time을 복원하는 보수적 구현을 채택했다.
- 즉, semantics는 TSDB mount 복원 쪽을 만족하지만, persistence 전략은 아직 upstream과 1:1은 아니다.

### 2.2 format / append / multi-sector write
구현된 내용:
- `TsDb::format()` 추가
- `TsDb::append(timestamp, payload)` 추가
- variable blob mode 기준 dual-ended 배치 사용
  - index는 sector header 뒤에서 앞으로 증가
  - payload는 sector 끝에서 뒤로 감소
- timestamp monotonic 정책 반영
  - `TimestampPolicy::StrictMonotonic`
  - `TimestampPolicy::AllowEqual`
- sector 공간 부족 시 현재 sector를 `FULL`로 전이하고 다음 sector로 이동
- non-rollover 구성에서는 더 이상 sector가 없으면 `Error::NoSpace` 반환 준비 완료

원본 FlashDB 비교 메모:
- upstream `write_tsl`, `tsl_append`, `update_sec_status`가 사용하는 dual-ended append 아이디어를 그대로 유지했다.
- 이번 slice에서는 rollover ring 정책까지는 아직 연결하지 않았고, 순차 sector advance까지만 구현했다.

### 2.3 forward iteration / reboot 복원 검증
구현된 내용:
- `TsOwnedRecord { timestamp, payload }` 추가
- `TsIterator` 추가
- `TsDb::iter()` 추가
- append 후 정방향 timestamp order 순회 가능
- reboot 후 `mount()`를 다시 호출해도 append된 레코드가 같은 순서로 복원됨

의미:
- plan 06의 방법론에서 권장한 "append path + forward iter 먼저" 단계가 실제 코드로 시작되었다.
- reverse/range query보다 앞서 correctness를 먼저 확보한 상태다.

### 2.4 현재 의도적으로 남겨둔 범위
이번 slice에서 아직 하지 않은 것:
- reverse iteration
- iter_by_time / query_count
- status mutation
- clean/reset 전용 API
- rollover=true ring overwrite 정책
- fixed-size blob mode
- append 시 sector header `end_info[0/1]`를 upstream처럼 증분 갱신하는 최적화

이 항목들은 plan 06 안에서 다음 phase로 이어서 구현해야 한다.

## 3. 이번 slice에서 수정된 파일

### 코드
- `src/tsdb/db.rs`
- `src/tsdb/iter.rs`
- `src/tsdb/mod.rs`

### 테스트
- `tests/ts_basic.rs`

### 문서
- `docs/plans/progress.md`

## 4. 테스트로 검증된 것

이번 slice는 TDD로 시작했다.

먼저 실패를 확인한 테스트:
- `cargo test --test ts_basic`
  - 초기에는 `TsDb::mount(...)` 자체가 없어 실패함을 확인

이후 구현 후 통과한 검증:
- `cargo test --test ts_basic`
- `cargo fmt`
- `cargo test`
- `cargo test --features std`

직접 검증된 핵심 시나리오:
- variable TSDB append가 timestamp 순서대로 기록됨
- 단일 DB lifetime에서 forward iteration이 timestamp order를 유지함
- sector 경계를 넘는 append 후 current sector가 다음 sector로 이동함
- reboot 후 mount가 oldest/current/last_timestamp를 복원함
- strict monotonic timestamp policy가 equal/older timestamp를 거부함

## 5. plan 06 현재 완료 판단

현재는 plan 06의 "첫 구현 slice 완료"로 판단한다.

구체적으로 완료된 phase:
- Phase 1. TSDB runtime state 정의: 부분 완료
- Phase 2. sector header / index mount logic: 부분 완료
- Phase 3. append 구현: variable mode 기준 부분 완료
- Phase 4. sector close / rollover 구현: sector close + next sector 이동만 부분 완료
- Phase 5. forward iteration 구현: 완료

아직 미완료라서 plan 06 전체 완료로 보지 않는 이유:
- reverse iteration이 없음
- time-range query / query_count가 없음
- rollover ring 정책이 없음
- clean/status mutation/fixed mode가 없음
- sector header `end_info` 증분 갱신도 아직 생략 상태임

## 6. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/06-tsdb-plan.md` 계속 진행
   - 다음 slice는 reverse iteration + range query(scan 기반) + query_count
2. 그 다음 rollover 정책과 clean/status mutation
3. plan 06이 충분히 닫히면 `docs/plans/07-testing-validation-and-rust-integration.md`

권장 구현 순서 메모:
- 먼저 `fdb_tsdb.c`의 `fdb_tsl_iter_reverse`, `search_start_tsl_addr`, `fdb_tsl_iter_by_time`, `fdb_tsl_query_count`를 다시 대조하면서
  full-scan 기반 correctness 버전을 붙이는 것이 가장 안전하다.
- 그 다음 sector header coarse filtering과 rollover 정책을 붙이는 것이 좋다.

## 7. 다음 세션 시작용 한 줄 요약

- "plan 06 진행 중. TSDB는 variable mode 기준 mount/format/append/forward iter와 reboot 복원까지 구현됐다. 다음은 reverse iter, time-range query/query_count, rollover 정책이다."