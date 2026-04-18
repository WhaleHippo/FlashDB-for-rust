# Blob and Codec Layer Plan

> 목적: 원본 `fdb_blob`을 Rust/Embassy에 맞는 Blob/Locator/Reader/Codec 구조로 재설계하고, 이후 KVDB/TSDB가 공통 payload 계층을 사용할 수 있게 만든다.

## 1. 목표

이 문서는 raw payload를 어떻게 다룰지에 대한 구현 계획이다.
원본 FlashDB의 blob은 매우 얇고 실용적이지만, Rust에서는 역할 분리를 통해 API 안정성과 no_std 친화성을 높이는 것이 중요하다.

핵심 목표:
- raw payload와 flash 위치 정보를 분리한다.
- 읽기/쓰기 API를 `&[u8]`, `&mut [u8]` 기반으로 정리한다.
- truncate/exact/partial read semantics를 명확히 한다.
- 상위에서 구조화 데이터 codec을 붙일 수 있게 한다.

## 2. 제안 계층

### 2.1 Blob 계층
- `BlobRef<'a>`: 쓰기용 borrowed payload
- `BlobBuf<'a>`: 읽기용 caller-provided mutable buffer

### 2.2 Locator 계층
- `BlobLocator`
- `KvValueLocator`
- `TsPayloadLocator`

### 2.3 Reader 계층
- `read_exact`
- `read_truncated`
- `read_chunk`
- `BlobCursor` 또는 `BlobChunkIter`

### 2.4 Codec 계층
- `Encode` / `Decode` 또는 비슷한 내부 trait
- optional helper for primitive/struct payload

## 3. 예상 파일

- `src/blob/mod.rs`
- `src/blob/locator.rs`
- `src/blob/reader.rs`
- `src/blob/codec.rs`
- `tests/blob_layer.rs`
- `tests/blob_codec.rs`

## 4. 세부 구현 단계

### Phase 1. BlobRef와 BlobBuf 정의

목표:
- 쓰기 입력과 읽기 버퍼를 분리한다.

후보 타입:
- `BlobRef<'a> { bytes: &'a [u8] }`
- `BlobBuf<'a> { bytes: &'a mut [u8] }`

검증 포인트:
- 길이 조회 가능
- empty blob 허용 여부 정책 결정
- no_std에서 추가 allocation 없이 사용 가능

### Phase 2. BlobLocator 정의

목표:
- 원본 `saved.meta_addr`, `saved.addr`, `saved.len`를 Rust 타입으로 캡슐화한다.

후보 타입:
- `BlobLocator`
  - `meta_offset`
  - `data_offset`
  - `len`

권장 정책:
- constructor는 private 또는 restricted
- scan/decode 함수만 locator 생성
- region bounds를 생성 시 검증

추가 타입:
- `KvValueLocator(BlobLocator)`
- `TsPayloadLocator(BlobLocator)`

장점:
- KV와 TS payload의 의미 차이를 타입으로 드러낼 수 있다.

### Phase 3. read semantics 분리

목표:
- exact/truncated/len query를 별도 API로 분리한다.

필수 API 초안:
- `blob_len(locator) -> usize`
- `read_blob_exact(locator, &mut [u8]) -> Result<(), _>`
- `read_blob_truncated(locator, &mut [u8]) -> Result<usize, _>`
- `read_blob_chunk(locator, offset, &mut [u8]) -> Result<usize, _>`

검증 포인트:
- buffer가 너무 작은 경우
- offset이 len을 넘는 경우
- chunk read가 정확히 끝나는 경우

### Phase 4. BlobCursor 또는 chunk iterator

목표:
- 큰 payload를 한 번에 전부 읽지 않고도 접근 가능하게 한다.

후보 타입:
- `BlobCursor`
- `BlobChunkIter<'a, F>`

권장 MVP 판단:
- MVP에서는 `read_chunk`만 먼저 구현해도 충분
- 이후 stream-like API를 얹는다

### Phase 5. codec trait 초안

목표:
- payload raw bytes와 typed value를 분리한다.

권장 구조:
- `trait EncodeToBytes`
- `trait DecodeFromBytes`
또는
- crate 내부 전용 codec trait

중요 원칙:
- DB core는 raw bytes만 안다.
- codec은 상위 utility layer에 둔다.

권장 helper 예시:
- `set_encoded<T: EncodeToBytes>(key, &T)`
- `get_decoded<T: DecodeFromBytes>(key) -> Result<T, _>`

주의:
- struct를 그대로 transmute-style로 bytes화하지 않는다.
- endian과 version 문제는 codec layer가 책임진다.

### Phase 6. TSDB fixed/variable payload 모드 연계

목표:
- TSDB에서 fixed-size blob mode와 variable mode가 Blob layer와 잘 맞도록 설계한다.

결정할 것:
- `BlobMode::Fixed(usize)`
- `BlobMode::Variable`

영향 받는 곳:
- TS index layout
- payload locator 계산
- chunk/cursor offset 계산

### Phase 7. 향후 확장 포인트 예약

이 단계에서 구현하진 않더라도 interface를 막지 말아야 하는 것:
- large chunked blob
- payload checksum
- compression
- encryption

원칙:
- core DB에 넣지 말고 blob policy/codec 계층에서 확장 가능하게 둘 것

## 5. 방법론

### 5.1 원본 `fdb_blob`를 1:1 이식하지 않는다
원본 의미는 참고하되, 역할이 섞인 구조는 분리한다.

### 5.2 payload는 raw bytes, 타입은 codec이 담당
- DB core는 타입을 모른다.
- Blob은 payload를 저장/읽는 수단이다.
- 구조체 해석은 codec 계층이 담당한다.

### 5.3 큰 payload를 염두에 두되 MVP는 단순하게
- 처음부터 chunk manifest까지 가면 복잡도가 커진다.
- MVP는 contiguous payload + partial read로 충분하다.

### 5.4 no_std 우선 설계
- owned buffer가 꼭 필요하면 `heapless` 기반 옵션 고려
- 기본 API는 borrowed slice 중심

## 6. 참고 자료

우선 참고:
- `docs/flashdb-architecture-analysis.md`
  - Blob 관련 섹션
- `docs/plans/02-storage-layout-and-status-foundation.md`

원본 코드:
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - `struct fdb_blob`
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - `fdb_blob_make`, `fdb_blob_read`
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - `fdb_kv_to_blob`, `fdb_kv_get_blob`
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - `fdb_tsl_to_blob`

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/inc/fdb_def.h`
  - 원본 `fdb_blob` 구조가 어떤 정보를 한 구조체에 몰아넣었는지 확인하는 출발점이다.
- `~/Desktop/FlashDB/src/fdb_utils.c`
  - blob이 단순 버퍼 descriptor로 쓰이는 방식과 `fdb_blob_read`의 truncation semantics를 확인할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
  - KV value를 blob locator로 바꾸는 흐름(`fdb_kv_to_blob`, `fdb_kv_get_blob`)을 보고 Rust의 `KvValueLocator` 설계를 정리할 때 참고한다.
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
  - TSL payload locator(`fdb_tsl_to_blob`)와 TS payload가 KV payload와 어떻게 다른지 비교할 때 참고한다.

## 7. 완료 기준

이 단계가 끝나면 다음이 가능해야 한다.
- KV/TS에서 공통적으로 locator를 리턴할 수 있다.
- caller는 payload 전체 길이를 알 수 있다.
- exact/truncated/partial read를 선택적으로 사용할 수 있다.
- raw bytes와 typed codec이 분리되어 있다.

## 8. 다음 문서

- `04-kvdb-mvp-plan.md`
- `06-tsdb-plan.md`
