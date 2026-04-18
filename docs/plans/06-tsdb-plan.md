# TSDB Plan

> 목적: FlashDB-for-rust의 TSDB를 구현하고, time-series append/query/reverse iteration/rollover를 안정적으로 제공한다.

## 1. 목표

TSDB 구현 목표:
- append-only time-series log
- timestamp ordered storage
- forward iteration
- reverse iteration
- range iteration by time
- query count by status/time
- status mutation
- clean/reset
- rollover 정책

핵심은 FlashDB 원본의 sector 내부 dual-ended allocation을 유지하는 것이다.

## 2. 범위

### 포함
- TS sector header
- index/data 양방향 배치
- current sector 관리
- oldest sector 계산
- append
- iter/iter_reverse
- iter_by_time
- query_count
- set_status
- clean

### 선택적 조기 반영
- fixed-size blob mode

### 제외
- 고급 인덱싱 최적화
- payload checksum 확장
- multi-writer 정책

## 3. 예상 파일

- `src/tsdb/mod.rs`
- `src/tsdb/db.rs`
- `src/tsdb/append.rs`
- `src/tsdb/query.rs`
- `src/tsdb/recovery.rs`
- `src/tsdb/iter.rs`
- `tests/ts_basic.rs`
- `tests/ts_query.rs`
- `tests/ts_rollover.rs`

## 4. 세부 구현 단계

### Phase 1. TSDB runtime state 정의

핵심 state:
- current sector
- oldest sector
- last timestamp
- max payload len
- rollover policy
- blob mode(fixed/variable)

주의:
- current sector와 oldest sector 계산이 깨지면 전체 query semantics가 무너진다.

### Phase 2. sector header / index mount logic

목표:
- mount 시 각 sector를 읽고 current/oldest를 판별한다.

읽어야 할 정보:
- sector store status
- start_time
- end_info[0/1]
- empty index position
- empty data position

결정할 것:
- invalid sector 발견 시 정책
- formatable / not-formatable 모드 지원 여부

### Phase 3. append 구현

목표:
- 현재 sector에 새 TSL을 append한다.

절차:
1. timestamp 정책 확인
2. payload 길이 확인
3. remain 공간 확인
4. 필요 시 sector close + next sector 선택
5. index PRE_WRITE 기록
6. index metadata 기록
7. payload write
8. index WRITE commit
9. current sector runtime metadata 갱신

핵심 검증 포인트:
- time strictly increasing
- sector 경계 직전 append
- fixed/variable mode 둘 다 위치 계산 정확성

### Phase 4. sector close / rollover 구현

목표:
- sector가 가득 찼을 때 end_info를 기록하고 다음 sector로 전환한다.

핵심 동작:
- 마지막 index/time metadata 기록
- current sector FULL 처리
- next sector 계산
- rollover=true면 ring처럼 순환
- rollover=false면 full 시 에러 반환

주의:
- next sector가 기존 데이터 보유 중이면 format 정책을 명확히 해야 함
- oldest sector 갱신을 정확히 해야 함

### Phase 5. forward iteration 구현

목표:
- oldest sector부터 current sector까지 time order로 순회한다.

구현 포인트:
- sector-level traversal
- sector 안 index 순차 순회
- current sector는 runtime metadata 우선 사용 가능

검증 포인트:
- 단일 sector
- 다중 sector
- empty sector 포함

### Phase 6. reverse iteration 구현

목표:
- current sector의 최신 데이터부터 역순 조회

구현 포인트:
- end_idx 기준 역방향 이동
- sector도 뒤로 이동
- empty/unused sector 처리 정책 정리

### Phase 7. iter_by_time / query_count 구현

목표:
- 시간 범위 기반 조회를 제공한다.

핵심 전략:
- sector header의 `start_time`, `end_time`으로 coarse filtering
- sector 내부는 index 기반 search

권장 순서:
1. 먼저 전체 scan 기반 range query 구현
2. 이후 sector filtering 추가
3. 필요 시 sector 내부 binary-search 유사 최적화 추가

이유:
- correctness를 먼저 확보하고 이후 최적화하는 편이 안전하다.

### Phase 8. status mutation 구현

목표:
- `WRITE`, `USER_STATUS1`, `DELETED` 등 상태 전이를 지원한다.

주의:
- TSDB 상태 변경도 status table 기반 단방향 write 제약을 따라야 함
- 허용 전이 표를 먼저 문서화하고 구현할 것

### Phase 9. clean/reset 구현

목표:
- 전체 sector format으로 DB를 초기화한다.

검증 포인트:
- clean 후 count = 0
- 재부팅 후에도 비어 있어야 함

### Phase 10. fixed-size blob mode 도입 여부 판단

권장 방향:
- variable mode 먼저 구현
- 그 다음 fixed-size mode 최적화 추가

이유:
- variable mode가 더 일반적이고 구현 논리도 명확함
- fixed mode는 layout helper가 준비된 뒤 붙이는 편이 쉬움

## 5. 방법론

### 5.1 append path와 query path를 분리 개발
- 먼저 append + forward iter
- 다음 reverse
- 마지막 range query

### 5.2 current/oldest sector 계산 테스트를 집중 작성
TSDB 버그는 대부분 여기서 발생한다.
mount logic에 대한 독립 테스트가 필요하다.

### 5.3 sector filtering은 최적화로 취급
처음부터 너무 공격적으로 최적화하지 않는다.
먼저 전체 scan이 정확한지 검증하고 그 다음 범위 최적화를 적용한다.

### 5.4 timestamp policy를 코드와 테스트에 명시
- strict monotonic이면 같은 timestamp를 거부
- 이후 정책을 바꿀 수 있도록 enum/config로 분리

## 6. 참고 자료

원본 핵심 참고:
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - `read_tsl`
  - `read_sector_info`
  - `write_tsl`
  - `update_sec_status`
  - `tsl_append`
  - `fdb_tsl_iter`
  - `fdb_tsl_iter_reverse`
  - `search_start_tsl_addr`
  - `fdb_tsl_iter_by_time`
  - `fdb_tsl_query_count`
  - `fdb_tsl_set_status`
  - `fdb_tsl_clean`
  - `fdb_tsdb_init`

보조 참고:
- `docs/flashdb-architecture-analysis.md`
- `docs/plans/02-storage-layout-and-status-foundation.md`
- `docs/plans/03-blob-and-codec-layer.md`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - `read_tsl`: index decode와 fixed/variable payload 위치 계산의 기준 구현이다.
  - `read_sector_info`: current/oldest sector 계산을 위한 sector metadata 해석 기준이다.
  - `write_tsl`, `update_sec_status`, `tsl_append`: append 경로와 sector full 처리, rollover 직전 동작을 참고한다.
  - `fdb_tsl_iter`, `fdb_tsl_iter_reverse`: 정/역순 iteration semantics를 설계할 때 참고한다.
  - `search_start_tsl_addr`, `fdb_tsl_iter_by_time`: range query와 sector 내부 search 전략을 구현할 때 참고한다.
  - `fdb_tsl_query_count`, `fdb_tsl_set_status`, `fdb_tsl_clean`: query/status/clean API 범위를 정할 때 참고한다.
  - `fdb_tsdb_init`: mount 시 current/oldest/last_time 복원 흐름의 기준으로 참고한다.
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - `fdb_tsl`, `tsdb_sec_info`, `fdb_tsl_status_t`의 의미를 cross-check할 때 보조 참조한다.

## 7. 완료 기준

다음 조건을 만족하면 TSDB 단계 완료로 본다.
- append 후 forward iter 정상
- reverse iter 정상
- time-range query 정상
- rollover on/off 정책 정상
- clean/reset 정상
- reboot 후 current/oldest/last_time 복원 가능

## 8. 다음 문서

- `07-testing-validation-and-rust-integration.md`
