# Storage, Layout, and Status Foundation Plan

> 목적: FlashDB-for-embassy의 가장 중요한 공통 기반인 storage region, 정렬 규칙, 상태 테이블, 온플래시 header codec을 먼저 안정화한다.

## 1. 목표

이 문서는 실제 DB 기능(KV/TSDB)보다 먼저 구현해야 하는 foundation을 다룬다.
여기서 흔들리면 이후 recovery, GC, query 전부가 무너진다.

핵심 질문:
- flash의 어떤 영역을 DB가 사용한다고 어떻게 표현할 것인가?
- write/erase granularity를 어떻게 안전하게 캡슐화할 것인가?
- 상태 전이 메타데이터를 어떻게 codec으로 구현할 것인가?
- 온플래시 header를 raw struct가 아니라 어떻게 decode/encode할 것인가?

## 2. 범위

이 문서 범위에 포함:
- storage region abstraction
- aligned read/write helper
- status table codec
- common constants/magic
- KV/TS sector header codec
- KV record header codec
- TS index codec
- CRC helper

이 문서 범위에서 제외:
- KV find/set/delete 정책
- GC 정책
- TS query 정책
- Blob high-level codec

## 3. 세부 구현 단계

### Phase 1. storage region 타입 정의

목표:
- DB가 사용하는 flash subrange를 타입으로 표현한다.

예상 파일:
- `src/storage/region.rs`
- `tests/storage_region.rs`

핵심 항목:
- `start`
- `len`
- `erase_size`
- `write_size`

강제할 불변식:
- `start % erase_size == 0`
- `start % write_size == 0`
- `len % erase_size == 0`
- `len >= erase_size * 2`

검증 포인트:
- invalid region 생성 시 에러
- valid region 생성 시 sector count 계산 가능

### Phase 2. logical offset <-> physical address 변환 helper

목표:
- DB 내부는 region-relative offset 중심으로 사고하고, backend는 절대 offset 또는 device offset으로 변환한다.

예상 파일:
- `src/storage/nor_flash.rs`
- `tests/storage_offset_map.rs`

필수 helper:
- `contains(offset, len)`
- `to_absolute(offset)`
- `sector_start(sector_index)`
- `sector_index_of(offset)`

주의:
- 이 레이어에서 주소 오버플로우/범위 초과를 막아야 한다.

### Phase 3. alignment helper

목표:
- FlashDB 원본의 `FDB_ALIGN`, `FDB_WG_ALIGN`, `FDB_WG_ALIGN_DOWN`에 대응되는 Rust helper를 만든다.

예상 파일:
- `src/layout/align.rs`
- `tests/align.rs`

필수 함수:
- `align_up(value, align)`
- `align_down(value, align)`
- `align_to_write_size(value, write_size)`
- `aligned_tail_size(len, write_size)`

검증 포인트:
- 1, 8, 32, 64 byte write size 모두 케이스 테스트
- 경계값 테스트: 0, 1, align-1, align, align+1

### Phase 4. status table codec

목표:
- 원본 `_fdb_set_status`, `_fdb_get_status`, `_fdb_write_status`, `_fdb_read_status`에 대응되는 codec 레이어를 만든다.

예상 파일:
- `src/layout/status.rs`
- `tests/status_codec.rs`

필수 개념:
- 상태 개수
- write granularity별 status table 길이 계산
- 상태 index -> bytes encode
- bytes -> 현재 상태 decode

권장 타입:
- `StatusScheme`
- `StatusTableBuf`
- `StatusCodec`

검증 포인트:
- KV 상태 enum 개수
- TSL 상태 enum 개수
- sector store/dirty 상태 개수
- write granularity별 원본과 동일한 상태 해석

특히 중요:
- 1bit/1byte/4byte/8byte 이상의 패턴이 원본 의미와 동일해야 한다.

### Phase 5. CRC helper

목표:
- 원본과 동일한 CRC32 계산을 보장한다.

예상 파일:
- `src/crc.rs`
- `tests/crc_compat.rs`

검증 포인트:
- known vector 테스트
- 원본 FlashDB와 동일 입력 동일 출력 확인
- padding 포함 CRC helper 제공

추가 권장 helper:
- `crc_with_ff_padding(data, aligned_len)`
- `crc_chain(parts: &[&[u8]])`

### Phase 6. common on-flash constants

목표:
- magic, erased byte, format version, 공통 상수를 한곳에 모은다.

예상 파일:
- `src/layout/common.rs`
- `tests/layout_common.rs`

후보 항목:
- erased byte
- data unused sentinel
- KV sector magic
- KV record magic
- TS sector magic
- format version

주의:
- sentinel과 실제 valid 값의 충돌 가능성을 문서화할 것

### Phase 7. KV header codec

목표:
- KV sector header와 KV record header를 명시적으로 encode/decode한다.

예상 파일:
- `src/layout/kv.rs`
- `tests/layout_kv.rs`

필수 타입 후보:
- `KvSectorHeader`
- `KvRecordHeader`
- `KvRecordLayout`

필수 기능:
- encode into byte buffer
- decode from byte buffer
- field validity check
- record total length 계산
- value start offset 계산
- CRC 범위 계산 helper

검증 포인트:
- name/value 길이에 따른 전체 레코드 길이 계산
- malformed length/header 처리
- write size별 padding 계산

### Phase 8. TS header/index codec

목표:
- TS sector header와 log index 구조를 명시적으로 구현한다.

예상 파일:
- `src/layout/ts.rs`
- `tests/layout_ts.rs`

필수 타입 후보:
- `TsSectorHeader`
- `TsEndInfo`
- `TsIndexHeader`
- `TsBlobMode`

필수 기능:
- variable blob mode encode/decode
- fixed blob mode index size 계산
- sector remain 계산 helper
- index/data dual-ended 배치 계산 helper

검증 포인트:
- fixed/variable mode 모두 위치 계산 테스트
- sector full 경계 테스트
- start/end index metadata decode 검증

### Phase 9. aligned write helper

목표:
- write-size 미정렬 logical write를 안전한 physical write로 바꾼다.

예상 파일:
- `src/storage/nor_flash.rs` 또는 `src/storage/write_aligned.rs`
- `tests/aligned_write.rs`

필수 기능:
- full aligned chunk 직접 write
- tail chunk는 erased-byte 채운 임시 buffer로 write

검증 포인트:
- size가 write_size 배수인 경우
- tail만 있는 경우
- header+payload 조합 시 위치가 안 깨지는지

주의:
- NOR semantics 위반(0->1 변화)을 mock에서 감지해야 한다.

## 4. 방법론

### 4.1 구현 순서는 storage -> align -> status -> codec
이 순서를 지키는 것이 좋다.
이유는 KV/TS codec도 결국 align과 status 규칙 위에 올라가기 때문이다.

### 4.2 테스트는 단위 테스트 중심
이 단계에서는 integration test보다 unit test가 중요하다.
각 helper는 입력/출력 표를 만들 수 있어야 한다.

### 4.3 원본 C와 바이트 단위 호환성을 직접 비교
가능하면 테스트에 아래 개념을 반영한다.
- 같은 field 입력
- 같은 write size
- 같은 padding
- 같은 status index
- 같은 CRC 결과

### 4.4 decode는 항상 방어적으로
decode는 다음을 분리해서 처리한다.
- byte parsing 실패
- 값은 읽혔지만 semantic invalid
- power-loss 중간 상태

## 5. 자주 참고해야 하는 원본 자료

가장 중요:
- `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
- `~/Desktop/FlashDB/src/fdb_utils.c`
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `struct sector_hdr_data`
  - `struct kv_hdr_data`
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - `struct sector_hdr_data`
  - `struct log_idx_data`

보조 참고:
- `~/Desktop/FlashDB/inc/fdb_def.h`
- `docs/flashdb-architecture-analysis.md`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
  - align 매크로, erased byte, status table 크기 계산식, low-level 함수 선언을 확인할 때 핵심 참조 파일이다.
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - `_fdb_set_status`, `_fdb_get_status`, `_fdb_write_status`, `_fdb_read_status`, `_fdb_flash_write_align` 구현을 그대로 비교 대상으로 삼는다.
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - KV sector header와 KV record header의 실제 필드 순서, padding, offset 계산을 확인할 때 사용한다.
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - TS sector header와 TSL index layout, fixed/variable blob 모드 분기 구조를 확인할 때 사용한다.
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - status enum 개수, public 구조체 필드 의미, sentinel 성격을 cross-check할 때 사용한다.

## 6. 완료 기준

이 단계가 끝나면 다음이 가능해야 한다.

- 임의의 region 설정을 validation할 수 있다.
- 상태 테이블을 encode/decode할 수 있다.
- KV/TS header를 바이트 배열로 encode/decode할 수 있다.
- aligned write helper를 mock flash에서 검증할 수 있다.
- 이후 KVDB/TSDB 구현이 “정책 문제”로만 남아야 한다.

## 7. 다음 문서

- `03-blob-and-codec-layer.md`
- `04-kvdb-mvp-plan.md`
