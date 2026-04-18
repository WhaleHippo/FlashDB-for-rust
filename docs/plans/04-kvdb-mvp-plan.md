# KVDB MVP Plan

> 목적: FlashDB-for-rust의 첫 번째 실사용 가능한 milestone으로 KVDB MVP를 구현한다.

## 1. 목표

MVP 범위의 KVDB는 다음을 제공해야 한다.

- mount/init
- format
- set(key, value)
- get(key)
- delete(key)
- scan 기반 lookup
- CRC 기반 torn-write recovery

이 단계에서는 FlashDB 원본의 모든 고급 기능을 한 번에 구현하지 않는다.
우선 “안전하게 저장되고, 재부팅 후 복구되며, 읽고 쓸 수 있는 KVDB”를 만든다.

## 2. 범위

### 포함
- append-only KV record
- key/value payload 저장
- tombstone 또는 delete 상태 기록
- boot scan
- CRC 검증
- write cursor 계산

### 제외
- sector dirty 기반 full GC
- default KV
- auto-update
- cache
- iterator 고도화
- integrity check API

## 3. 예상 파일

- `src/kv/mod.rs`
- `src/kv/db.rs`
- `src/kv/scan.rs`
- `src/kv/write.rs`
- `src/kv/recovery.rs`
- `tests/kv_basic.rs`
- `tests/kv_recovery.rs`

## 4. 세부 구현 단계

### Phase 1. KV runtime state 정의

목표:
- mount 이후 유지할 runtime state를 정의한다.

후보 필드:
- region
- current write cursor
- next sequence 또는 scan cursor
- format version
- last valid record 위치

결정 사항:
- MVP에서 원본과 동일한 sector status 체계를 즉시 반영할지
- 또는 먼저 linear append region으로 단순화할지

권장:
- FlashDB 철학을 유지하려면 sector 경계는 초기에 반영
- 다만 allocation 정책은 단순화 가능

### Phase 2. boot scan 구현

목표:
- DB mount 시 flash를 순회하여 마지막 일관된 상태를 찾는다.

핵심 동작:
- region start부터 순차 scan
- header decode
- len 검증
- bounds 검증
- CRC 검증
- invalid header/CRC가 나오면 power-loss tail로 간주하고 scan 종료

scan 결과:
- write cursor
- latest record index 또는 lookup 가능 상태
- recovery 필요 여부

검증 포인트:
- 빈 flash mount
- 한 개 record 있는 flash mount
- 중간 write에서 끊긴 flash mount

### Phase 3. set(key, value) append 구현

목표:
- 새 KV record를 append한다.

절차:
1. key/value 길이 검증
2. record total length 계산
3. 남은 공간 확인
4. header 작성
5. key/value aligned write
6. final commit status 기록
7. runtime cursor 갱신

주의:
- CRC에 padding 포함 여부를 원본과 동일하게 유지
- write 순서는 recovery 친화적으로 구성

검증 포인트:
- string key + small blob value
- binary blob value
- 최대 경계 근처 길이

### Phase 4. get(key) scan lookup 구현

목표:
- 최신 유효 record를 찾아 value를 읽는다.

전략:
- MVP에서는 linear scan 허용
- newest wins semantics 유지
- deleted/tombstone record가 최신이면 not found

API 수준에서 분리 권장:
- `get_locator(key)`
- `get_blob_into(key, buf)`
- `contains_key(key)`

검증 포인트:
- 같은 key를 여러 번 set
- set -> delete
- delete 후 재set

### Phase 5. delete(key) 구현

목표:
- in-place erase가 아니라 append된 delete record 또는 delete status로 처리한다.

선택지:
1. 원본과 가까운 상태 전이 기반 삭제
2. tombstone record append

권장:
- Rust MVP에서는 tombstone append 방식이 단순하고 검증이 쉬움
- 단, 이후 FlashDB 원본 semantics에 더 가깝게 재편 가능한 구조로 설계

검증 포인트:
- 기존 key 삭제
- 없는 key 삭제 시 정책
- delete 후 get은 not found

### Phase 6. recovery 처리

목표:
- PRE_WRITE 또는 CRC 실패 꼬리 구간을 mount 시 안전하게 버린다.

MVP recovery 정책:
- 마지막 일관된 record까지만 유효
- 그 뒤는 버림

이 단계에서 중요한 점:
- recovery는 “손상 복구”보다 “안전한 cut-off”가 우선
- cut-off가 정확하면 이후 재쓰기 가능

### Phase 7. format 구현

목표:
- region 전체 erase + runtime reset

검증 포인트:
- format 후 mount 시 빈 DB
- format 후 set/get 정상
- 이전 데이터 접근 불가

## 5. 방법론

### 5.1 KVDB MVP는 반드시 단순해야 한다
MVP에서 너무 많은 FlashDB 기능을 한 번에 넣지 않는다.
먼저 “append + scan + recovery”가 안정되어야 한다.

### 5.2 lookup 성능보다 correctness 우선
- 초반엔 O(n) scan 허용
- cache는 이후 단계로 미룸

### 5.3 delete semantics를 일찍 고정
- tombstone 모델을 쓸지
- 원본 PRE_DELETE/DELETED semantics 일부를 바로 반영할지
작업 초기에 결정해야 한다.

권장 초기 방향:
- API는 tombstone-like
- 내부 상태기는 이후 원본 호환 쪽으로 확장 가능하게 설계

### 5.4 integration test 우선
이 단계는 unit test보다 DB 시나리오 테스트가 중요하다.
필수 시나리오:
- empty -> set -> get
- set -> overwrite -> get latest
- set -> delete -> not found
- interrupted write -> reboot -> old data intact

## 6. 참고 자료

우선 참고:
- `docs/flashdb-architecture-analysis.md`
- `docs/plans/02-storage-layout-and-status-foundation.md`
- `docs/plans/03-blob-and-codec-layer.md`

원본 코드 핵심 참고:
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `read_kv`
  - `find_kv`
  - `create_kv_blob`
  - `set_kv`
  - `_fdb_kv_load`
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - CRC / aligned write helper

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `read_kv`: header decode, CRC 검증, name/value 위치 계산의 기준 구현으로 참고한다.
  - `find_kv`: scan 기반 lookup 흐름과 latest-wins semantics를 참고한다.
  - `create_kv_blob`: record 생성 순서, CRC 범위, key/value write 순서를 비교할 때 본다.
  - `set_kv`: overwrite/delete/update의 상위 흐름을 참고한다.
  - `_fdb_kv_load`: mount 시 boot scan과 recovery cut-off 지점을 잡는 방식의 기준으로 본다.
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - CRC 계산과 aligned tail write를 그대로 비교하면서 Rust helper를 구현한다.
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - KV status enum과 public KV object 의미를 cross-check할 때 보조 참조한다.

## 7. 완료 기준

다음 조건을 만족하면 MVP 완료로 본다.

- mock flash에서 set/get/delete roundtrip 성공
- 재부팅 후 mount해도 데이터 일관성 유지
- interrupted write 이후 mount 가능
- corrupted tail 이후에도 이전 valid record 읽기 가능
- API가 Blob locator/read 모델과 자연스럽게 연결됨

## 8. 후속 연결

MVP가 끝나면 바로 다음 문서로 넘어간다.
- `05-kvdb-gc-and-recovery-plan.md`
