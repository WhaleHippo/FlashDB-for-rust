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
- TSDB 쪽은 variable mode 기준으로 mount/append/forward iteration/reverse iteration/time-range query/query_count/status mutation/clean-reset까지 올라온 상태다.

## 2. 이번에 plan 06에서 완료한 것

plan 06은 아직 전부 끝난 것은 아니지만, 이번 기준에서는 correctness 우선의 세 번째 TSDB slice까지 구현했다.

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
- 다만 현재 Rust 구현은 sector header의 `end_info`를 append 때마다 갱신하는 방식까지는 아직 구현하지 않았고,
  mount 시 index area를 다시 스캔해서 current/oldest/last_time을 복원하는 보수적 구현을 채택했다.
- 즉, mount/recovery semantics는 만족하지만 persistence 최적화는 아직 upstream과 1:1이 아니다.

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
- non-rollover 구성에서는 더 이상 sector가 없으면 `Error::NoSpace` 반환

원본 FlashDB 비교 메모:
- upstream `write_tsl`, `tsl_append`, `update_sec_status`가 사용하는 dual-ended append 아이디어를 그대로 유지했다.
- 이번 기준에서도 rollover ring 정책까지는 아직 연결하지 않았고, 순차 sector advance까지만 구현했다.

### 2.3 forward iteration / reboot 복원
구현된 내용:
- `TsOwnedRecord { status, timestamp, payload }` 추가
- `TsIterator` 추가
- `TsDb::iter()` 추가
- append 후 정방향 timestamp order 순회 가능
- reboot 후 `mount()`를 다시 호출해도 append된 레코드가 같은 순서로 복원됨

의미:
- plan 06의 권장 구현 순서인 "append path + forward iter 먼저" 단계가 완료되었다.

### 2.4 reverse iteration + iter_by_time + query_count
이전 slice에서 완료한 것:
- `TsDb::iter_reverse()` 추가
- `TsDb::iter_by_time(from, to)` 추가
- `TsDb::query_count(from, to, status)` 추가

현재 구현 정책:
- 먼저 전체 snapshot scan으로 correctness를 확보하는 구현을 채택했다.
- `iter_reverse()`는 전체 record snapshot을 reverse하여 최신 timestamp부터 반환한다.
- `iter_by_time(from, to)`는 inclusive 범위로 동작한다.
  - `from <= to`이면 forward range iteration
  - `from > to`이면 reverse range iteration
- `query_count(from, to, status)`는 같은 inclusive 범위 안에서 status가 일치하는 항목 수를 센다.

원본 FlashDB 비교 메모:
- upstream `fdb_tsl_iter_reverse`, `search_start_tsl_addr`, `fdb_tsl_iter_by_time`, `fdb_tsl_query_count`를 참고했다.
- 하지만 현재 Rust 구현은 sector header coarse filtering / sector 내부 시작점 탐색 최적화 대신,
  full snapshot scan + filter 방식으로 correctness를 먼저 맞춘 상태다.
- 즉, API semantics는 plan 06의 다음 단계와 맞추되, 탐색 최적화는 아직 후속 작업이다.

### 2.5 status mutation + clean/reset
이번 slice에서 새로 완료한 것:
- `TSL_USER_STATUS1`, `TSL_USER_STATUS2` 상수 추가
- `TsDb::set_status(timestamp, status)` 추가
- `TsDb::clean()` 추가
- iterator/query가 `TSL_WRITE`뿐 아니라 상태가 바뀐 record도 그대로 노출/집계하도록 조정

현재 구현 정책:
- `set_status(timestamp, status)`는 timestamp로 record를 찾아 상태 테이블을 추가 프로그래밍한다.
- 현재는 strict monotonic timestamp 정책을 쓰는 기본 테스트 구성을 전제로 하므로 timestamp가 사실상 고유 키 역할을 한다.
- 상태 전이는 monotonic 방향만 허용한다.
  - 예: `WRITE -> USER_STATUS1`, `WRITE -> DELETED` 허용
  - 역방향 전이는 거부
- `clean()`은 전체 DB를 다시 format하는 안전한 reset wrapper로 구현했다.

원본 FlashDB 비교 메모:
- upstream `fdb_tsl_set_status`도 index status table에 추가 프로그래밍하는 방식이라, 핵심 메커니즘은 동일한 방향이다.
- upstream `fdb_tsl_clean`은 내부적으로 전체 sector format을 수행하는데, 현재 Rust 구현도 그 의미를 `format()` 재사용으로 맞췄다.
- 다만 현재 public API는 upstream의 `fdb_tsl_t` 핸들 기반이 아니라 timestamp 기반 lookup을 사용한다는 차이가 있다.
- 이 차이는 현재 Rust 코드베이스에서 검증 가능한 단순한 API를 우선 택한 결과이며, 향후 더 직접적인 record handle API로 바꿀 수 있다.

### 2.6 현재 의도적으로 남겨둔 범위
이번 기준에서 아직 하지 않은 것:
- rollover=true ring overwrite 정책
- fixed-size blob mode
- append 시 sector header `end_info[0/1]`를 upstream처럼 증분 갱신하는 최적화
- range query에 대한 sector header coarse filtering / binary-search 유사 최적화
- upstream에 더 가까운 record-handle 기반 status mutation surface

이 항목들은 plan 06 안에서 다음 phase로 이어서 구현해야 한다.

## 3. 이번 slice에서 수정된 파일

### 코드
- `src/layout/ts.rs`
- `src/tsdb/db.rs`
- `src/tsdb/iter.rs`

### 테스트
- `tests/ts_basic.rs`

### 문서
- `docs/plans/progress.md`

## 4. 테스트로 검증된 것

이번 slice도 TDD로 진행했다.

먼저 실패를 확인한 테스트:
- `cargo test --test ts_basic`
  - 처음에는 `TSL_USER_STATUS1`, `set_status`, `clean`이 없어 실패함을 확인

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
- reverse iteration이 최신 timestamp부터 역순으로 record를 반환함
- time-range query가 inclusive bounds로 동작함
- `from > to`인 `iter_by_time`이 reverse range iteration으로 동작함
- `query_count`가 status/time 조건을 함께 적용해 개수를 반환함
- `set_status(20, TSL_USER_STATUS1)` 이후 iterator/query 결과가 새 status를 반영함
- `clean()` 이후 모든 record가 사라지고 DB가 다시 append 가능한 초기 상태로 돌아감
- clean 후 reboot해도 새로 쓴 record만 남음

## 5. plan 06 현재 완료 판단

현재는 plan 06의 "세 번째 구현 slice 완료"로 판단한다.

구체적으로 완료된 phase:
- Phase 1. TSDB runtime state 정의: 부분 완료
- Phase 2. sector header / index mount logic: 부분 완료
- Phase 3. append 구현: variable mode 기준 부분 완료
- Phase 4. sector close / rollover 구현: sector close + next sector 이동만 부분 완료
- Phase 5. forward iteration 구현: 완료
- Phase 6. reverse iteration 구현: 완료
- Phase 7. iter_by_time / query_count 구현: correctness-first 버전 완료
- Phase 8. status mutation 구현: timestamp lookup 기반 버전 완료
- Phase 9. clean/reset 구현: 완료

아직 미완료라서 plan 06 전체 완료로 보지 않는 이유:
- rollover ring 정책이 없음
- fixed-size blob mode가 없음
- sector header `end_info` 증분 갱신이 없음
- range query의 sector-level filtering / search 최적화가 없음
- status mutation surface가 upstream의 record-handle 방식과는 다름

## 6. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/06-tsdb-plan.md` 계속 진행
   - 다음 slice는 rollover 정책 + oldest/current sector 갱신 고도화
2. 그 다음 fixed blob mode
3. 마지막으로 query 최적화와 `end_info` 증분 갱신
4. plan 06이 충분히 닫히면 `docs/plans/07-testing-validation-and-rust-integration.md`

권장 구현 순서 메모:
- 먼저 `fdb_tsdb.c`의 sector full 이후 next sector 선택과 oldest sector 갱신 흐름을 다시 대조하면서,
  rollover=true일 때 ring처럼 재사용하는 정책을 구현하는 것이 좋다.
- 그 다음 fixed blob mode를 붙이고,
- 마지막으로 sector header coarse filtering / 내부 search 최적화를 추가하는 편이 자연스럽다.

## 7. 다음 세션 시작용 한 줄 요약

- "plan 06 진행 중. TSDB는 variable mode 기준 mount/format/append/forward iter/reverse iter/time-range query/query_count/status mutation/clean-reset과 reboot 복원까지 구현됐다. 다음은 rollover 정책과 fixed blob mode다."