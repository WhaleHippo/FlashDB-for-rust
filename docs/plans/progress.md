# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 작업 중인 plan: `docs/plans/05-kvdb-gc-and-recovery-plan.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 진행 중 (Phase 1/2/4/5/8/9에 해당하는 첫 구현 slice 반영)
  - plan 06 이후: 아직 미구현

즉, 현재 프로젝트는:
- storage/alignment/status/layout foundation을 이미 확보했고,
- blob abstraction / locator / codec 계층이 준비되어 있으며,
- KVDB MVP의 mount/init, format, set/get/delete, scan lookup, torn-write/CRC tail recovery가 동작하고,
- 그 위에 plan 05의 recovery/dirty metadata/traversal/integrity 관련 첫 slice가 추가된 상태다.

## 2. 이번에 plan 05에서 완료한 것

### 2.1 PRE_DELETE 기반 상태기계 첫 반영
`src/kv/db.rs`, `src/kv/scan.rs`, `src/kv/recovery.rs`를 중심으로 overwrite/delete 경로를 FlashDB 쪽 상태 전이에 더 가깝게 보강했다.

구현된 내용:
- overwrite/delete 전에 old record를 `KV_PRE_DELETE`로 전이
- 새 record append 이후 old record를 `KV_DELETED`로 finalize
- mount/lookup/traversal에서 `KV_PRE_DELETE`를 recovery 가능한 live 상태로 해석
- PRE_DELETE만 남은 중간 상태에서도 기존 값이 계속 읽히도록 처리

의미:
- plan 04의 단순 tombstone append-only semantics에서 한 단계 올라가,
- update/delete 중 전원 차단을 더 자연스럽게 해석할 수 있는 기초 상태기계가 들어왔다.

### 2.2 sector metadata / dirty tracking 가시화
`src/kv/recovery.rs`, `src/kv/write.rs`, `src/kv/scan.rs`, `src/kv/db.rs`를 통해 sector 상태를 읽고 관찰할 수 있게 했다.

구현된 내용:
- sector header의 store status를 write path에서 `EMPTY -> USING -> FULL` 방향으로 갱신
- overwrite/delete 시 old record가 있던 sector를 dirty로 마킹
- `KvDb::sector_meta(sector_index)` 추가
  - `store_status`
  - `dirty_status`
  - `next_record_offset`
  - `remaining_bytes`

의미:
- 아직 GC 본체는 없지만,
- 어떤 sector가 dirty 후보인지와 남은 공간이 얼마인지 런타임에서 직접 관찰 가능해졌다.

### 2.3 live traversal API 추가
`src/kv/db.rs`에 traversal surface를 추가했다.

구현된 내용:
- `KvDb::for_each_live_record(key_buf, value_buf, visit)` 추가
- stale record / deleted tombstone은 숨기고
- latest/live record만 callback으로 노출

의미:
- plan 05의 iterator/traversal 요구를 첫 단계로 충족한다.
- 아직 dedicated iterator struct는 없지만, 테스트와 상위 로직이 live set을 순회할 수 있다.

### 2.4 integrity check API 추가
`src/kv/scan.rs`, `src/kv/db.rs`에 전체 KVDB 정합성 점검 API를 추가했다.

구현된 내용:
- `KvDb::check_integrity()` 추가
- sector header decode 실패 수 집계
- record header 손상 / 길이 이상 / CRC mismatch 수 집계
- `KvIntegrityReport { sector_issues, record_issues }`
- `is_clean()` 헬퍼 제공

의미:
- recovery 이후 상태를 눈으로 검증할 수 있는 디버깅/시뮬레이션용 진단면이 생겼다.

## 3. 이번 slice에서 수정된 파일

### 코드
- `src/kv/db.rs`
- `src/kv/mod.rs`
- `src/kv/recovery.rs`
- `src/kv/scan.rs`
- `src/kv/write.rs`

### 테스트
- `tests/kv_plan05.rs`

### 문서
- `docs/plans/progress.md`

## 4. 테스트로 검증된 것

이번 상태에서 다음 명령이 통과했다.

- `cargo fmt`
- `cargo test`
- `cargo test --features std`

새로 검증된 핵심 시나리오:
- overwrite 후 old sector dirty status 반영
- sector metadata에서 next write 위치 / remaining bytes 확인
- PRE_DELETE만 남은 중간 상태에서도 mount 후 기존 값 유지
- traversal API가 stale/deleted record를 숨기고 live/latest record만 노출
- integrity check가 손상된 record header를 검출

## 5. plan 05 완료 판단

아직 plan 05 전체 완료는 아니다.

이번 기준에서 완료된 범위:
- 상태기계의 첫 구체화
- PRE_DELETE 해석 및 recovery-friendly lookup
- dirty sector tracking의 시작점
- traversal API의 첫 구현
- integrity check API

아직 남은 핵심 범위:
- `src/kv/gc.rs`
  - 실제 copy-forward GC
  - GC victim 선정 정책
  - reclaim 후 공간 회수 경로
- recovery state machine의 추가 고도화
  - old/new record 관계를 더 upstream 가깝게 정리하는 mount-time 정리
- dedicated iterator struct / 보다 풍부한 traversal surface
- optional cache
- default KV / auto update 설계 반영

## 6. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/05-kvdb-gc-and-recovery-plan.md` 계속 진행
   - 특히 Phase 6 `copy-forward GC 구현`
   - 이어서 Phase 7 `GC 정책 분리`
2. 이후 `docs/plans/06-tsdb-plan.md`

이유:
- dirty metadata와 traversal/integrity surface가 생겨서,
- 이제 실제 GC를 구현하고 반복 set/delete 후 공간을 회수하는 단계로 넘어갈 준비가 됐다.

## 7. 다음 세션 시작용 한 줄 요약

- "plan 05는 부분 진행 상태다. PRE_DELETE 기반 overwrite/delete 상태 전이, dirty sector metadata, live traversal API, integrity check API까지 구현 및 테스트 완료. 다음은 실제 GC(copy-forward + reclaim policy) 구현이다."