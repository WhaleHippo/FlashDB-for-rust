# FlashDB-for-rust 진행 현황

작성일: 2026-04-19
이 문서는 현재 구현 상태를 한 번에 파악할 수 있도록 유지하는 snapshot이다.

## 1. 현재 기준점

- 현재 막 완료한 plan: `docs/plans/05-kvdb-gc-and-recovery-plan.md`
- 전체 진행 위치:
  - plan 00: 해석 완료
  - plan 01: 완료
  - plan 02: 완료
  - plan 03: 완료
  - plan 04: 완료
  - plan 05: 완료
  - plan 06 이후: 아직 미구현

즉, 현재 프로젝트는:
- storage/alignment/status/layout foundation을 이미 확보했고,
- blob abstraction / locator / codec 계층이 준비되어 있으며,
- KVDB MVP의 mount/init, format, set/get/delete, scan lookup, torn-write/CRC tail recovery가 동작하고,
- 그 위에 plan 05의 recovery/dirty metadata/GC/iterator/integrity 요구사항까지 연결된 상태다.

## 2. 이번에 plan 05에서 완료한 것

### 2.1 PRE_DELETE 기반 상태기계 보강
`src/kv/db.rs`, `src/kv/scan.rs`, `src/kv/recovery.rs`를 중심으로 overwrite/delete 경로를 FlashDB 쪽 상태 전이에 더 가깝게 보강했다.

구현된 내용:
- overwrite/delete 전에 old record를 `KV_PRE_DELETE`로 전이
- 새 record append 이후 old record를 `KV_DELETED`로 finalize
- mount/lookup/traversal에서 `KV_PRE_DELETE`를 recovery 가능한 live 상태로 해석
- PRE_DELETE만 남은 중간 상태에서도 기존 값이 계속 읽히도록 처리

의미:
- plan 04의 단순 tombstone append-only semantics에서 한 단계 올라가,
- update/delete 중 전원 차단을 더 자연스럽게 해석할 수 있는 상태기계가 들어왔다.

### 2.2 sector metadata / dirty tracking 정교화
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
- GC 전/후 sector 상태를 런타임에서 직접 관찰 가능하다.

### 2.3 GC 구현 및 자동 공간 회수 연결
`src/kv/gc.rs`, `src/kv/db.rs`, `src/kv/write.rs`, `src/kv/iter.rs`를 통해 garbage collection 경로를 실제 동작하게 만들었다.

구현된 내용:
- `KvDb::collect_garbage()` 추가
- live set snapshot을 기반으로 region을 재포맷한 뒤 live record만 재기록하는 compacting GC 구현
- 새 record append 전 공간 부족이 예상되면 GC를 먼저 수행하도록 연결
- repeated overwrite 후에도 최신 값만 유지하며 계속 기록 가능
- GC 후 dirty sector 상태가 정리되고 free space가 다시 확보됨

정책 메모:
- 이번 구현의 GC는 upstream FlashDB의 sector-victim copy-forward를 그대로 복제하기보다,
  현재 Rust 코드베이스에서 검증 가능한 방식으로 “live set compacting GC”를 채택했다.
- plan 05 완료 기준(공간 회수, dirty 정리, live set 보존)은 만족한다.

### 2.4 iterator / traversal API 확장
`src/kv/iter.rs`, `src/kv/db.rs`, `src/kv/mod.rs`를 통해 dedicated iterator surface를 추가했다.

구현된 내용:
- `KvOwnedRecord { key, value }`
- `KvIterator`
- `KvDb::iter()`
- 기존 `for_each_live_record(...)` 유지
- stale record / deleted tombstone은 숨기고 latest/live record만 iterator에 노출

의미:
- plan 05의 iterator 요구를 충족한다.
- 테스트와 상위 로직이 live set snapshot을 직접 순회할 수 있다.

### 2.5 integrity check API 유지/검증 강화
`src/kv/scan.rs`, `src/kv/db.rs`에 추가된 전체 KVDB 정합성 점검 API를 유지하고, plan 05 완료 기준에 맞춰 검증 시나리오를 확장했다.

구현된 내용:
- `KvDb::check_integrity()`
- sector header decode 실패 수 집계
- record header 손상 / 길이 이상 / CRC mismatch 수 집계
- `KvIntegrityReport { sector_issues, record_issues }`
- `is_clean()` 헬퍼 제공

의미:
- recovery/GC 이후 상태를 테스트/시뮬레이션에서 바로 점검할 수 있다.

## 3. 이번 slice에서 수정된 파일

### 코드
- `src/kv/db.rs`
- `src/kv/gc.rs`
- `src/kv/iter.rs`
- `src/kv/mod.rs`
- `src/kv/write.rs`
- `src/lib.rs`

### 테스트
- `tests/kv_plan05.rs`

### 문서
- `docs/plans/progress.md`

## 4. 테스트로 검증된 것

이번 상태에서 다음 명령이 통과했다.

- `cargo fmt`
- `cargo test`
- `cargo test --features std`

plan 05 완료와 직접 연결되는 핵심 시나리오:
- overwrite 후 old sector dirty status 반영
- sector metadata에서 next write 위치 / remaining bytes 확인
- PRE_DELETE만 남은 중간 상태에서도 mount 후 기존 값 유지
- traversal API가 stale/deleted record를 숨기고 live/latest record만 노출
- integrity check가 손상된 record header를 검출
- repeated overwrite cycle에서 GC가 자동으로 개입하여 최신 값 유지 + 쓰기 계속 가능
- manual GC 후 dirty sector가 정리되고 live record만 유지됨
- iterator snapshot이 live record만 반환함

## 5. plan 05 완료 판단

이번 기준에서 plan 05 완료로 판단한 이유:
- repeated set/delete/overwrite 후 공간 회수가 가능하다.
- dirty sector가 GC 이후 정리된다.
- power-loss recovery 시 old/new 관계가 PRE_DELETE 해석을 통해 깨지지 않는다.
- iterator가 live KV만 노출한다.
- integrity check가 손상 record를 검출한다.

보수적으로 보면 앞으로 더 고도화할 수 있는 영역은 있다.
예를 들면:
- sector-victim 기반 세밀한 copy-forward 정책
- optional cache
- default KV / auto update 설계

하지만 이건 plan 05 완료의 필수 조건이 아니라 이후 최적화/확장 대상로 보는 것이 맞다.

## 6. 다음 작업 우선순위

가장 추천하는 다음 단계:
1. `docs/plans/06-tsdb-plan.md`
2. 이후 `docs/plans/07-testing-validation-and-rust-integration.md`

이유:
- KVDB 쪽은 MVP + recovery/GC/iterator/integrity까지 갖춰졌고,
- 이제 TSDB 구현으로 넘어갈 수 있는 상태가 되었다.

## 7. 다음 세션 시작용 한 줄 요약

- "plan 05까지 완료됐다. KVDB는 PRE_DELETE 기반 recovery, sector metadata/dirty tracking, compacting GC, iterator snapshot, integrity check까지 구현 및 테스트 완료. 다음은 plan 06 TSDB 구현이다."