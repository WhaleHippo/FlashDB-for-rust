# KVDB GC and Recovery Plan

> 목적: KVDB MVP 위에 FlashDB다운 공간 회수, dirty sector 관리, recovery, iterator, cache를 단계적으로 추가한다.

## 1. 목표

이 문서는 MVP 이후 KVDB를 원본 FlashDB에 더 가깝게 만드는 단계다.
핵심은 아래 두 축이다.

1. power-loss 이후 중간 상태를 복구하는 상태기계
2. 삭제/갱신 후 쌓이는 garbage를 sector 단위로 회수하는 GC

## 2. 범위

### 포함
- sector store/dirty 상태 정교화
- PRE_WRITE / PRE_DELETE / DELETED recovery
- dirty sector tracking
- copy-forward GC
- iterator
- integrity check
- optional cache
- default KV / auto update 설계 착수

### 제외
- full wear leveling
- cross-region migration
- advanced cache tuning

## 3. 예상 파일

- `src/kv/recovery.rs`
- `src/kv/gc.rs`
- `src/kv/iter.rs`
- `src/kv/cache.rs` (선택)
- `tests/kv_gc.rs`
- `tests/kv_recovery_advanced.rs`
- `tests/kv_iterator.rs`

## 4. 세부 구현 단계

### Phase 1. 상태기계 구체화

목표:
- MVP 수준의 append/tombstone semantics를 원본 FlashDB와 더 가까운 상태 전이 구조로 보강한다.

정리 대상 상태:
- UNUSED
- PRE_WRITE
- WRITE
- PRE_DELETE
- DELETED
- ERR_HDR

작업 내용:
- 상태 전이 허용표 작성
- recovery 시 각 상태를 어떻게 해석할지 표준화
- 테스트 케이스와 1:1 매핑

### Phase 2. sector metadata 정교화

목표:
- sector 단위 상태를 명확히 계산하고 유지한다.

관리 항목:
- store status: EMPTY / USING / FULL
- dirty status: FALSE / TRUE / GC
- remain bytes
- next empty record 위치

중요 포인트:
- sector 상태는 flash에서 읽은 값과 runtime cache 값이 어긋나지 않아야 한다.
- cache가 없어도 scan으로 항상 복원 가능해야 한다.

### Phase 3. PRE_WRITE recovery

목표:
- 중간에 쓰다 끊긴 record를 mount 시 안전하게 정리한다.

정책:
- header만 있고 payload/CRC가 완성되지 않은 레코드는 invalid tail로 처리
- 필요 시 ERR_HDR 상태로 마킹
- write cursor를 마지막 valid 지점으로 이동

검증 시나리오:
- header만 씀
- header + key 일부만 씀
- payload 일부만 씀
- final commit 전에 전원 차단

### Phase 4. PRE_DELETE recovery

목표:
- update/delete 도중 전원 차단 시 old record/new record 관계를 정리한다.

원본 참고 포인트:
- old KV를 PRE_DELETE로 바꾸고
- new KV를 만든 뒤
- old KV를 최종 DELETED로 바꿈

복구 시 고려할 것:
- old record만 남아 있는 경우
- new record가 이미 committed된 경우
- 둘 다 부분 상태인 경우

이 부분은 가장 위험한 영역이므로 테스트를 많이 작성한다.

### Phase 5. dirty sector tracking

목표:
- 삭제나 갱신이 일어난 sector를 dirty로 표시한다.

핵심 요구:
- delete/update 후 dirty status가 설정되어야 함
- GC 대상 sector를 판별 가능해야 함
- scan 시 dirty 상태를 복원 가능해야 함

### Phase 6. copy-forward GC 구현

목표:
- dirty sector에서 살아 있는 record만 새 공간으로 이동하고 sector를 format한다.

절차:
1. GC victim 선정
2. sector dirty status를 GC로 전이
3. live record 판별
4. 새 sector로 copy-forward
5. 원 sector erase/format
6. oldest/current metadata 갱신

중요 원칙:
- 항상 최소 1개 empty sector를 작업 공간으로 보장
- GC 중 전원 차단에도 mount 가능한 구조 유지

### Phase 7. GC 정책 분리

목표:
- GC 메커니즘과 “언제/어떤 sector를 수거할지” 정책을 분리한다.

후보 정책:
- first dirty sector
- lowest free-space sector
- oldest dirty sector

MVP 이후 첫 구현은 단순 정책이면 충분하다.
하지만 policy abstraction은 남겨둔다.

### Phase 8. iterator / traversal API

목표:
- 모든 live KV를 순회할 수 있게 한다.

후보 API:
- `iter()`
- `iter_keys()`
- `iter_records()`

주의:
- deleted/invalid record는 숨김
- recovery/GC 이후에도 동일한 iterator semantics 유지

### Phase 9. integrity check API

목표:
- 전체 KVDB를 scan하며 record/header/CRC 정합성을 확인한다.

이 API의 용도:
- 테스트 디버깅
- simulator validation
- hardware smoke diagnostics

### Phase 10. cache 도입 여부 평가

목표:
- correctness가 아니라 성능 최적화로서 cache를 추가할지 판단한다.

후보:
- key lookup cache
- sector meta cache

원칙:
- cache miss 시 scan으로 항상 복원 가능
- cache는 optional feature 또는 내부 구현 detail로 유지

### Phase 11. default KV / auto update 설계 반영

목표:
- 원본 FlashDB의 default KV와 version-based auto update 기능을 Rust 방식으로 옮길지 결정한다.

초기 방향:
- 바로 구현하지 않아도 됨
- config/API 설계를 먼저 문서화

## 5. 방법론

### 5.1 recovery와 GC는 반드시 crash test와 함께 개발
이 영역은 normal path만 봐서는 안 된다.
각 phase마다 “중간에 끊기면?” 시나리오를 테스트에 넣어야 한다.

### 5.2 live record 판별 로직을 함수로 고정
GC의 핵심은 무엇이 살아 있는지 판별하는 것이다.
이 로직을 여러 군데 복붙하지 않는다.

### 5.3 GC는 먼저 단순하고 명확하게
너무 똑똑한 victim 선택보다, 재현 가능하고 테스트 쉬운 정책이 우선이다.

### 5.4 iterator는 recovery/GC 이후 semantics를 기준으로 테스트
순회 API는 단순해 보이지만 stale record를 노출하기 쉽다.
“사용자가 봐야 하는 live set”만 노출되는지 계속 검증한다.

## 6. 참고 자료

원본 핵심 참고:
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `del_kv`
  - `move_kv`
  - `do_gc`
  - `gc_collect_by_free_size`
  - `check_and_recovery_gc_cb`
  - `check_and_recovery_kv_cb`
  - `fdb_kv_iterator_init`
  - `fdb_kv_iterate`
  - `fdb_kvdb_check`

보조 참고:
- `docs/flashdb-architecture-analysis.md`
- `docs/plans/04-kvdb-mvp-plan.md`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `del_kv`: PRE_DELETE/DELETED 전이와 dirty sector 마킹 방식을 확인할 때 본다.
  - `move_kv`: copy-forward GC의 핵심 동작과 recovery 시 record 이동 로직을 확인할 때 본다.
  - `do_gc`, `gc_collect_by_free_size`: GC victim 처리, empty sector 확보, oldest_addr 갱신 흐름을 확인할 때 본다.
  - `check_and_recovery_gc_cb`, `check_and_recovery_kv_cb`: 전원 차단 후 recovery state machine의 기준으로 사용한다.
  - `fdb_kv_iterator_init`, `fdb_kv_iterate`: iterator semantics를 설계할 때 참고한다.
  - `fdb_kvdb_check`: integrity check API가 어떤 수준까지 검사해야 하는지 참고한다.
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - sector dirty/store status enum과 iterator 구조체 의미를 확인할 때 보조 참조한다.

## 7. 완료 기준

다음 조건을 만족하면 이 단계 완료로 본다.

- repeated set/delete 후 공간 회수가 가능
- dirty sector가 GC로 정리됨
- power-loss recovery 시 old/new record 관계가 깨지지 않음
- iterator가 live KV만 순회함
- integrity check가 손상 record를 검출 가능

## 8. 다음 문서

- `06-tsdb-plan.md`
- `07-testing-validation-and-rust-integration.md`
