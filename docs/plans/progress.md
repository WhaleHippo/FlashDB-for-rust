# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 막 완료한 plan: `docs/plans/04-kvdb-mvp-plan.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05 이후: 아직 미구현

즉, 현재 프로젝트는:
- storage/alignment/status/layout foundation을 이미 확보했고,
- blob abstraction / locator / codec 계층이 준비되어 있으며,
- 그 위에 KVDB MVP의 mount/init, format, set/get/delete, scan lookup, torn-write/CRC tail recovery까지 연결된 상태다.

## 2. 이번에 plan 04에서 완료한 것

### 2.1 KVDB runtime state와 mount/boot scan 구현
`src/kv/db.rs`, `src/kv/scan.rs`를 중심으로 KVDB runtime을 실제 동작 가능한 형태로 확장했다.

구현된 내용:
- `KvDb<F>` 제네릭 런타임 정의
- `KvDb::mount(flash, config)`
- region/layout validation
- boot scan으로 write cursor 계산
- sector header/record header decode 기반 sequential scan

의미:
- 기존의 설정-only placeholder에서 벗어나,
- 실제 flash backend를 붙여 mount 가능한 KVDB core가 생겼다.

### 2.2 format + append-only set/delete 경로 구현
`src/kv/write.rs`, `src/kv/recovery.rs`를 추가 구현했다.

구현된 내용:
- `format()`
  - 전 sector erase
  - sector header 재초기화
- `set(key, value)`
  - CRC 계산
  - PRE_WRITE header 기록
  - key/value aligned write
  - status commit
- `delete(key)`
  - tombstone-like append delete record
  - 최신 record 기준 delete semantics 반영
- status transition은 intermediate state까지 순차 프로그램하도록 구현

의미:
- MVP 단계에서 필요한 append-only write path와 delete path가 갖춰졌고,
- FlashDB status table의 write-once 제약을 지키는 방향으로 commit/recovery가 동작한다.

### 2.3 scan 기반 latest-wins lookup 구현
`src/kv/scan.rs`, `src/kv/db.rs`에 lookup surface를 추가했다.

구현된 내용:
- `get_locator(key)`
- `get_blob_into(key, buf)`
- `contains_key(key)`
- scan 기반 newest-wins lookup
- delete record가 최신이면 not found 처리

의미:
- MVP 요구사항인 O(n) scan lookup을 우선 구현했고,
- 이후 cache 최적화(plan 05) 없이도 올바른 read semantics를 제공한다.

### 2.4 recovery 처리 구현
`src/kv/recovery.rs`, `src/kv/scan.rs`에서 mount-time recovery를 연결했다.

구현된 내용:
- PRE_WRITE tail 검출 시 `KV_ERR_HDR`로 승격
- CRC mismatch tail 검출 시 `KV_ERR_HDR`로 승격
- 이후 write cursor를 안전한 다음 append 지점으로 이동
- recovery 이후에도 이전 valid record read 유지
- recovery 이후 새 write 가능

의미:
- 단순히 mount를 실패시키지 않고,
- MVP 수준에서 power-loss tail을 잘라내고 계속 운용 가능한 상태를 만든다.

## 3. 이번 slice에서 수정된 파일

### 코드
- `src/error.rs`
- `src/kv/db.rs`
- `src/kv/mod.rs`
- `src/kv/recovery.rs`
- `src/kv/scan.rs`
- `src/kv/write.rs`

### 테스트
- `tests/kv_basic.rs`
- `tests/kv_recovery.rs`

### 문서
- `docs/plans/README.md`
- `docs/plans/progress.md`

## 4. 테스트로 검증된 것

이번 상태에서 다음 명령이 통과했다.

- `cargo fmt`
- `cargo test`
- `cargo test --features std`

새로 검증된 핵심 시나리오:
- empty -> format -> set -> get
- overwrite 후 latest-wins lookup
- set -> delete -> not found -> re-set
- reboot 후 mount 복구
- PRE_WRITE tail recovery 후 이전 값 유지 + 재쓰기 가능
- CRC mismatch tail recovery 후 이전 값 유지 + 재쓰기 가능

## 5. plan 04 완료 판단

이번 기준에서 plan 04 완료로 판단한 이유:
- mount/init가 실제 flash backend 기준으로 동작한다.
- format이 region reset을 수행한다.
- set/get/delete roundtrip이 mock flash에서 검증되었다.
- scan 기반 lookup이 최신 record semantics를 제공한다.
- interrupted/corrupted tail 이후에도 mount 및 이전 데이터 유지가 가능하다.
- Blob locator/read 모델과 KV read surface가 자연스럽게 연결되었다.

보수적으로 보면 아직 GC, cache, iterator 고도화, sector dirty 기반 재정리 로직은 없다.
하지만 이건 plan 04 범위가 아니라 plan 05 이후 대상이다.

## 6. 아직 스캐폴드 또는 다음 plan 대상인 영역

이건 plan 04 미완료가 아니라 상위 plan의 미구현 영역이다.

- `src/kv/gc.rs`
  - sector reclaim / compact / dirty 상태 관리
- `src/kv/iter.rs`
  - iterator API 고도화
- cache / default KV / auto-update
- 보다 upstream에 가까운 sector lifecycle 및 recovery 정교화
- `src/tsdb/*`
  - TSDB 본 구현은 plan 06 대상

## 7. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/05-kvdb-gc-and-recovery-plan.md`
2. 이후 `docs/plans/06-tsdb-plan.md`

이유:
- KVDB MVP core가 준비되었고,
- 이제 append-only MVP를 실제 장기 운용 가능한 KVDB로 만들기 위해 GC/recovery/iterator 쪽을 확장할 시점이다.

## 8. 다음 세션 시작용 한 줄 요약

- "plan 04까지 완료됐다. KVDB MVP mount/init, format, set/get/delete, latest-wins scan lookup, PRE_WRITE/CRC tail recovery, reboot 후 재쓰기 가능한 경로까지 구현 및 테스트 완료. 다음은 plan 05 GC/recovery 고도화다."