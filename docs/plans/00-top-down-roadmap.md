# FlashDB-for-embassy Top-Down Roadmap

> 목적: FlashDB 개념을 Rust + Embassy 환경에 맞게 재구현하기 위한 전체 구현 순서를 고정하고, 이후 개별 task가 흔들리지 않도록 상위 방향을 문서화한다.

## 1. 최종 목표

최종 목표는 다음 세 가지를 동시에 만족하는 crate를 만드는 것이다.

1. `embedded-storage` 계열 NOR flash backend 위에서 동작한다.
2. FlashDB의 핵심 철학을 유지한다.
   - sector 기반 구조
   - write granularity 정렬
   - status table 기반 상태 전이
   - append-only 기록
   - boot-time recovery
3. Rust/Embassy답게 더 안전하고 확장 가능한 구조를 제공한다.
   - 명시적 codec
   - 명확한 error model
   - Blob/Locator 분리
   - 테스트 자동화 강화

## 2. 상위 아키텍처 원칙

### 2.1 유지할 것
- sector ring 개념
- KVDB의 copy-forward GC
- TSDB의 dual-ended sector layout
- power-loss tolerant 상태 전이
- file/mock 기반 host-side 검증 가능성

### 2.2 개선할 것
- raw struct write/read 제거
- C식 blob descriptor 분해
- 오류 분류 세분화
- 모듈 경계 명확화
- 테스트를 설계 단계에서 포함

### 2.3 초기에 하지 않을 것
- full wear leveling
- 대형 chunked blob
- encryption/compression policy
- 다중 format migration 체계
- aggressive caching optimization

## 3. 구현 단계 개요

### Phase 0. 문서/설계 고정
목표:
- 구현 전 모듈 구조, 포맷 경계, 책임 분리를 고정한다.

산출물:
- `docs/plans/*.md`
- 필요 시 추가 포맷 문서 `docs/flashdb-onflash-format.md`

완료 기준:
- 이후 task가 “어디에 무엇을 넣을지” 질문 없이 시작 가능해야 함

### Phase 1. crate 뼈대와 공통 규약
목표:
- Rust crate 기본 구조와 공통 타입을 만든다.

핵심 모듈:
- `src/lib.rs`
- `src/error.rs`
- `src/config.rs`
- `src/storage/`
- `src/layout/`

완료 기준:
- 빈 DB 구현 없이도 storage/layout/error가 독립적으로 컴파일됨

### Phase 2. storage + layout + status foundation
목표:
- flash 접근과 온플래시 메타데이터 해석의 기반을 만든다.

핵심 기능:
- region abstraction
- aligned write helper
- status table codec
- sector/record header codec
- CRC

완료 기준:
- unit test만으로 alignment/status/header encode-decode가 검증됨

### Phase 3. Blob/Locator/Codec 계층
목표:
- raw payload 처리 계층을 공통 모듈로 분리한다.

핵심 기능:
- `BlobRef`
- `BlobLocator`
- read exact / read truncated / len query
- codec layer 초안

완료 기준:
- KVDB/TSDB가 같은 Blob foundation을 공유할 수 있어야 함

### Phase 4. KVDB MVP
목표:
- 가장 먼저 쓸 수 있는 persistent KV 저장소를 만든다.

핵심 기능:
- mount/init
- format
- set/get/delete
- scan 기반 조회
- CRC 기반 torn-write recovery

완료 기준:
- mock flash 위에서 roundtrip과 reboot recovery test 통과

### Phase 5. KVDB 고도화
목표:
- FlashDB다운 KVDB로 확장한다.

핵심 기능:
- sector dirty tracking
- GC
- PRE_DELETE recovery
- default KV
- iterator
- integrity check
- cache(선택)

완료 기준:
- GC/recovery 시나리오 테스트 통과

### Phase 6. TSDB MVP + 고도화
목표:
- append-heavy time-series storage를 구현한다.

핵심 기능:
- sector dual-ended allocation
- append
- iter forward/reverse
- iter by time
- query count
- status update
- clean
- fixed-size mode(선택적 조기 반영 가능)

완료 기준:
- time-order, reverse, range query, rollover 동작 검증

### Phase 7. host simulation + crash test + hardware validation
목표:
- 실제 안정성을 보장한다.

핵심 기능:
- RAM mock flash
- file-backed flash simulator
- forced reboot / interrupted write test
- Embassy example
- STM32F302 hardware smoke test

완료 기준:
- host 환경과 실제 보드에서 동일한 핵심 시나리오 통과

## 4. 추천 작업 순서

실제 task 순서는 다음을 권장한다.

1. crate 구조 만들기
2. error/config/storage region 정의
3. alignment/status/CRC 구현
4. sector/KV/TS header codec 작성
5. Blob layer 작성
6. KVDB MVP 구현
7. KVDB GC/recovery 구현
8. TSDB 구현
9. simulator/test harness 강화
10. Embassy 예제와 문서 보완

## 5. 제안 파일/디렉터리 구조

```text
FlashDB-for-embassy/
├─ src/
│  ├─ lib.rs
│  ├─ error.rs
│  ├─ config.rs
│  ├─ crc.rs
│  ├─ storage/
│  │  ├─ mod.rs
│  │  ├─ region.rs
│  │  ├─ nor_flash.rs
│  │  ├─ mock.rs
│  │  └─ file_sim.rs
│  ├─ layout/
│  │  ├─ mod.rs
│  │  ├─ align.rs
│  │  ├─ status.rs
│  │  ├─ common.rs
│  │  ├─ kv.rs
│  │  └─ ts.rs
│  ├─ blob/
│  │  ├─ mod.rs
│  │  ├─ locator.rs
│  │  ├─ reader.rs
│  │  └─ codec.rs
│  ├─ kv/
│  │  ├─ mod.rs
│  │  ├─ db.rs
│  │  ├─ scan.rs
│  │  ├─ write.rs
│  │  ├─ recovery.rs
│  │  ├─ gc.rs
│  │  └─ iter.rs
│  └─ tsdb/
│     ├─ mod.rs
│     ├─ db.rs
│     ├─ append.rs
│     ├─ query.rs
│     ├─ recovery.rs
│     └─ iter.rs
├─ tests/
│  ├─ kv_basic.rs
│  ├─ kv_recovery.rs
│  ├─ kv_gc.rs
│  ├─ ts_basic.rs
│  ├─ ts_query.rs
│  └─ crash_scenarios.rs
├─ examples/
│  ├─ kv_mock.rs
│  └─ stm32f302_kv_demo.rs
└─ docs/
   ├─ flashdb-architecture-analysis.md
   └─ plans/
```

## 6. 방법론

### 6.1 문서 우선, 구현 후 보정
- 먼저 계획 문서를 기준으로 모듈 경계를 고정한다.
- 구현 중 경계가 바뀌면, 먼저 문서를 고친 뒤 코드에 반영한다.

### 6.2 TDD 우선
- header/status/alignment는 unit test 먼저
- DB API는 integration test 먼저
- recovery는 interrupted-write 시나리오 테스트 먼저

### 6.3 MVP와 고도화 분리
- 초기에는 cache/wear-leveling/large-blob 같은 복잡한 기능을 제외한다.
- 대신 확장 포인트를 코드 구조에 남긴다.

### 6.4 모듈 간 책임 강제
- storage는 flash primitive만 담당
- layout은 byte encoding/decoding만 담당
- kv/tsdb는 정책과 상태기계 담당
- blob은 payload abstraction 담당

## 7. 참고 자료 가이드

### 반드시 참고
- `docs/flashdb-architecture-analysis.md`
- 원본 FlashDB
  - `~/Desktop/FlashDB/inc/fdb_def.h`
  - `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
  - `~/Desktop/FlashDB/src/fdb_utils.c`
  - `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `~/Desktop/FlashDB/src/fdb_tsdb.c`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - 전체 공개 타입, 상태 enum, DB 구조체 정의를 이해할 때 가장 먼저 본다.
- `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
  - status table 크기 계산, align 매크로, low-level flash helper 개념을 볼 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - CRC32, status encode/decode, aligned write helper의 원형을 확인할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - KVDB의 전체 상태기계, GC, recovery, iterator 흐름을 상위 관점에서 파악할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - TSDB의 sector dual-ended allocation, append, query, mount 로직을 상위 관점에서 파악할 때 참고한다.

### 구현 중 수시 참고
- `embedded-storage` trait 문서
- Embassy synchronization 관련 문서
- 현재 보드 대상 flash write/erase granularity 정보

## 8. 각 단계 완료 시 남겨야 할 기록

각 phase를 끝낼 때 아래를 남긴다.

- 실제 구현된 파일 목록
- 의도와 달라진 설계 포인트
- 아직 미구현인 부분
- 다음 phase 시작 조건

권장 기록 위치:
- 기존 plan 문서 말미에 “Implementation Notes” 섹션 추가
또는
- `docs/plans/progress-YYYYMMDD.md` 신규 문서 작성

## 9. 이 문서 다음에 읽을 것

- `01-workspace-and-crate-structure.md`
- `02-storage-layout-and-status-foundation.md`
