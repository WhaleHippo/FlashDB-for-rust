# FlashDB-for-rust 구현 계획 인덱스

이 폴더는 FlashDB-for-rust를 실제로 구현하기 위한 top-down 계획 문서 모음이다.
각 문서는 독립적으로 읽을 수 있게 작성했지만, 실제 작업은 아래 순서를 기준으로 진행하는 것을 권장한다.

## 권장 읽기/실행 순서

1. `00-top-down-roadmap.md`
   - 전체 목표, 아키텍처 원칙, 구현 순서, 산출물 맵
2. `01-workspace-and-crate-structure.md`
   - crate 구조, 모듈 배치, 공통 규약
3. `02-storage-layout-and-status-foundation.md`
   - storage region, 정렬, 상태 테이블, 헤더 codec
4. `03-blob-and-codec-layer.md`
   - Blob/Locator/Codec 계층 설계
5. `04-kvdb-mvp-plan.md`
   - KVDB MVP 구현 순서
6. `05-kvdb-gc-and-recovery-plan.md`
   - KVDB GC, recovery, cache 고도화
7. `06-tsdb-plan.md`
   - TSDB 구현 순서
8. `07-testing-validation-and-rust-integration.md`
   - host simulation, crash test, hardware validation, Rust 예제

## 문서 사용 원칙

- 각 문서는 “다음 작업에서 바로 이어서 구현할 수 있는 수준”을 목표로 한다.
- 문서 안에는 다음이 포함된다.
  - 목표
  - 범위
  - 세부 단계
  - 예상 파일 경로
  - 방법론
  - 검증 방법
  - 참고해야 할 자료
- 실제 구현 시에는 한 문서를 끝까지 한 번에 다 하기보다, 문서 내 Phase/Step 단위로 나누어 진행한다.

## 공통 참고 문서

먼저 아래 문서를 읽고 시작하는 것을 권장한다.

- `../flashdb-architecture-analysis.md`
- 원본 FlashDB
  - `~/Desktop/FlashDB/inc/flashdb.h`
  - `~/Desktop/FlashDB/inc/fdb_def.h`
  - `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
  - `~/Desktop/FlashDB/src/fdb_utils.c`
  - `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `~/Desktop/FlashDB/src/fdb_tsdb.c`

## plan 문서별 원본 FlashDB 참조 파일 빠른 매핑

- `00-top-down-roadmap.md`
  - 전체 상위 구조: `inc/fdb_def.h`, `inc/fdb_low_lvl.h`, `src/fdb_utils.c`, `src/fdb_kvdb.c`, `src/fdb_tsdb.c`
- `01-workspace-and-crate-structure.md`
  - 공개 API/타입 경계: `inc/flashdb.h`, `inc/fdb_def.h`
  - 구현 분리 감각: `src/fdb_kvdb.c`, `src/fdb_tsdb.c`
- `02-storage-layout-and-status-foundation.md`
  - storage/status/alignment: `inc/fdb_low_lvl.h`, `src/fdb_utils.c`
  - header layout: `src/fdb_kvdb.c`, `src/fdb_tsdb.c`
- `03-blob-and-codec-layer.md`
  - blob 원형: `inc/fdb_def.h`, `src/fdb_utils.c`, `src/fdb_kvdb.c`, `src/fdb_tsdb.c`
- `04-kvdb-mvp-plan.md`
  - KV read/write/mount: `src/fdb_kvdb.c`, `src/fdb_utils.c`
- `05-kvdb-gc-and-recovery-plan.md`
  - KV recovery/GC/iterator: `src/fdb_kvdb.c`
- `06-tsdb-plan.md`
  - TS append/query/mount: `src/fdb_tsdb.c`
- `07-testing-validation-and-rust-integration.md`
  - 원본 테스트/시뮬레이션: `tests/fdb_kvdb_tc.c`, `tests/fdb_tsdb_tc.c`, `src/fdb_file.c`

## 진행 관리 권장 방식

각 구현 task를 시작할 때는 아래 정보를 남기면 연속성이 좋아진다.

- 현재 보고 있는 plan 파일
- 현재 phase / step 번호
- 수정 대상 파일
- 완료 기준
- 실제 완료 여부와 발견한 차이점

필요하면 이후 `docs/plans/progress-*.md` 형태의 진행 로그 문서를 따로 추가해도 된다.
