# FlashDB 구조 분석 보고서

작성 시각: 2026-04-18 20:00:13 KST
분석 대상 원본: `~/Desktop/FlashDB`
대상 워크스페이스: `~/Desktop/FlashDB-for-rust`
목적: STM32 + Rust 환경에서 동작하는 Rust 기반 FlashDB 재구현의 선행 분석 기록

## 1. 분석 목표

이 문서는 C로 작성된 FlashDB 원본 프로젝트의 전체 구조를 Rust 관점에서 재구현하기 위해 정리한 아키텍처 분석 보고서다.
초점은 다음 네 가지다.

1. 원본 프로젝트의 디렉터리/모듈 구조 파악
2. KVDB/TSDB의 온플래시 데이터 레이아웃과 상태 전이 파악
3. 초기화, 복구, GC, 순회, 포팅 계층의 핵심 동작 파악
4. Rust 포팅 시 유지해야 할 개념과 재설계가 필요한 지점 분리

이 문서는 이후 `FlashDB-for-rust` 구현 시 설계 기준 문서로 계속 확장하는 것을 전제로 작성한다.

## 2. 저장소 전체 구조 요약

원본 저장소의 핵심 상위 디렉터리는 아래와 같다.

- `inc/`
  - 공개 API, 공개 타입, low-level 공용 매크로/선언, 설정 템플릿
- `src/`
  - 공통 초기화/저수준 I/O/CRC/상태 처리 + KVDB/TSDB 본 구현
- `samples/`
  - KVDB/TSDB 사용 예제
- `tests/`
  - RT-Thread Utest 기반 테스트 코드
- `port/fal/`
  - Flash Abstraction Layer(FAL) 포팅 계층
- `demos/`
  - Linux, ESP, STM32 보드 데모
- `docs/`
  - 사용자 문서

핵심 파일 규모는 대략 다음과 같다.

- `src/fdb_kvdb.c`: 1944 lines
- `src/fdb_tsdb.c`: 1096 lines
- `src/fdb_utils.c`: 349 lines
- `src/fdb_file.c`: 317 lines
- `src/fdb.c`: 157 lines
- `inc/fdb_def.h`: 351 lines
- `tests/fdb_kvdb_tc.c`: 979 lines
- `tests/fdb_tsdb_tc.c`: 518 lines

결론적으로 FlashDB는 “작아 보이지만 실제 핵심 복잡도는 KVDB와 TSDB 각각의 상태 복구/순회/공간 관리 로직에 집중된 구조”다.

## 3. 소스 파일별 역할

### 3.1 `inc/flashdb.h`

사용자 공개 API 헤더다.

주요 책임:
- DB 초기화/제어/해제 API 제공
- KVDB API 제공
  - `fdb_kv_set`, `fdb_kv_get`, `fdb_kv_set_blob`, `fdb_kv_get_blob`, `fdb_kv_del`, `fdb_kv_get_obj`
- TSDB API 제공
  - `fdb_tsl_append`, `fdb_tsl_iter`, `fdb_tsl_iter_reverse`, `fdb_tsl_iter_by_time`, `fdb_tsl_query_count`, `fdb_tsl_set_status`, `fdb_tsl_clean`
- blob abstraction 제공
- CRC32 유틸 공개

Rust 포팅 시 이 파일은 사실상 crate의 public API surface에 해당한다.

### 3.2 `inc/fdb_def.h`

FlashDB 전체의 공개 타입 정의와 제어 커맨드, 상태 enum, DB 구조체가 몰려 있다.

핵심 내용:
- 버전 정보
- 설정 매크로 기본값
- 제어 커맨드 정의
- `fdb_kv_status_t`, `fdb_tsl_status_t`
- sector store/dirty 상태 정의
- `fdb_kv`, `fdb_tsl`, `fdb_blob`
- `fdb_db`, `fdb_kvdb`, `fdb_tsdb`
- sector info 구조체
- cache 구조체

이 파일은 Rust 관점에서 아래 3개 레이어로 분해하는 것이 자연스럽다.
- public types
- internal metadata types
- runtime state structs

### 3.3 `inc/fdb_low_lvl.h`

가장 중요한 공용 내부 헤더 중 하나다.

핵심 역할:
- write granularity 정렬 매크로 제공
- status table 크기 계산 매크로 제공
- erased/written byte 정의
- 저수준 flash read/write/erase 함수 선언
- status encode/decode 및 contiguous-FF 탐색 함수 선언

Rust 포팅 시 이 파일은 “storage trait + layout helper + status codec” 모듈로 거의 그대로 대응된다.

### 3.4 `inc/fdb_cfg_template.h`

사용자 설정 템플릿이다.

핵심 설정:
- `FDB_USING_KVDB`
- `FDB_USING_TSDB`
- `FDB_USING_FAL_MODE`
- `FDB_USING_FILE_LIBC_MODE` / `FDB_USING_FILE_POSIX_MODE`
- `FDB_WRITE_GRAN`
- `FDB_KV_AUTO_UPDATE`
- `FDB_TSDB_FIXED_BLOB_SIZE`

Rust 포팅에서는 C preprocessor 매크로 대신 아래로 치환하는 것이 적합하다.
- const generic
- feature flag
- builder/config struct

### 3.5 `src/fdb.c`

공통 DB 초기화 코드다.

핵심 역할:
- `_fdb_init_ex`: DB 공통 초기 검증
- `_fdb_init_finish`: 초기화 완료 처리 및 로그
- `_fdb_deinit`: file mode 리소스 해제
- `_fdb_db_path`: FAL partition 또는 file path 반환

중요 포인트:
- FAL 모드에서는 partition을 찾고 block size를 읽어 `sec_size` 정합성을 검증한다.
- `sec_size`는 block size 배수여야 한다.
- `max_size`는 `sec_size` 배수여야 한다.
- 최소 2개 sector가 있어야 한다.

즉, 실제 DB 로직보다 먼저 “영역 정합성”을 강하게 강제한다.

### 3.6 `src/fdb_utils.c`

CRC, status table, blob read, 공통 flash I/O wrapper 구현이다.

핵심 역할:
- `fdb_calc_crc32`
- `_fdb_set_status`, `_fdb_get_status`
- `_fdb_write_status`, `_fdb_read_status`
- `_fdb_continue_ff_addr`
- `fdb_blob_make`, `fdb_blob_read`
- `_fdb_flash_read`, `_fdb_flash_write`, `_fdb_flash_erase`
- `_fdb_flash_write_align`

특히 `_fdb_flash_write_align`는 Rust 포팅에서 매우 중요하다.
원본도 write granularity가 큰 플래시(STM32 internal flash 포함)를 고려해 정렬되지 않은 logical write를 alignment-safe physical write로 바꿔준다.
Rust/NorFlash 포팅에서도 반드시 같은 추상화가 필요하다.

### 3.7 `src/fdb_kvdb.c`

KVDB 전체 구현체다. 원본에서 가장 복잡한 파일이다.

책임 범위:
- sector/KV layout 정의
- cache
- read/write/delete
- sector allocation
- GC
- power-loss recovery
- default KV 초기화
- auto-update
- iterator
- integrity check

### 3.8 `src/fdb_tsdb.c`

TSDB 구현체다.

책임 범위:
- sector/TSL index layout 정의
- append 로직
- sector rollover
- forward/reverse/time-range iteration
- count query
- status update
- 전체 초기화/clean

### 3.9 `src/fdb_file.c`

file mode 백엔드다.

핵심 아이디어:
- 각 sector를 개별 파일로 매핑
- `db_name.fdb.<sector_index>` 형식 사용
- POSIX/LIBC 두 구현 제공
- 테스트/리눅스 데모에서 주로 사용

Rust 최종 목표에서는 필수는 아니지만, host-side 시뮬레이터 또는 integration test용 백엔드로 매우 유용하다.

## 4. FlashDB의 공통 아키텍처 개념

FlashDB는 단순한 추상 key-value 라이브러리가 아니라 “flash 특성을 직접 반영한 log-structured embedded database”다.

공통 개념은 다음과 같다.

### 4.1 Sector 기반 운영

모든 DB는 여러 개 sector로 구성된다.

- sector는 erase 최소 단위 또는 그 배수
- 포맷/GC/rollover의 기본 단위는 sector
- DB 전체는 sector ring처럼 순환 가능
- `oldest_addr` 개념으로 순회 시작점을 잡음

### 4.2 Write-once, status transition 기반 상태 관리

플래시는 일반적으로 erase 없이 1 -> 0 방향 쓰기만 안전하다.
FlashDB는 이를 적극 활용한다.

각 object/sector는 상태 테이블(status table)로 상태 전이를 표현한다.
즉,
- 상태 값을 덮어쓰는 대신
- “더 진행된 상태를 나타내는 비트/바이트를 추가로 0으로 기록”하는 방식이다.

이 구조 덕분에 power loss 중간 상태를 복구 가능하다.

### 4.3 Aligned write abstraction

원본은 `FDB_WRITE_GRAN`을 중심으로 모든 레이아웃을 정렬한다.

지원 write granularity 예시:
- 1 bit: 일반 NOR flash
- 8 bit: STM32F2/F4
- 32 bit: STM32F1
- 64 bit: STM32L4/F7 류
- 128/256 bit: 일부 고급 STM32 계열

Rust로 가면 이 값은 `embedded_storage::nor_flash::NorFlash::WRITE_SIZE`와 대응된다.

### 4.4 Storage backend 분리

원본은 두 storage mode를 가진다.

- FAL mode: 실제 flash partition 기반
- file mode: 파일 기반 시뮬레이션

즉 상위 DB 로직은 read/write/erase primitive만 믿고 동작한다.
Rust 포팅에서도 동일하게 storage backend와 DB 로직을 분리해야 한다.

## 5. 상태 테이블(status table) 설계

FlashDB의 가장 중요한 설계 포인트다.

### 5.1 기본 아이디어

`_fdb_set_status` / `_fdb_get_status`는 상태 enum을 직접 숫자로 저장하지 않는다.
대신 status table을 통해 “현재까지 진행된 상태 단계”를 표현한다.

장점:
- erase 없이 상태 전이 가능
- power loss 시 PRE_WRITE, PRE_DELETE 같은 중간 상태를 감지 가능
- 작은 metadata write로 상태 전환 가능

### 5.2 status 저장 방식

`FDB_WRITE_GRAN == 1`인 경우와 그 외 경우가 다르다.

- 1bit granularity:
  - 비트 패턴으로 상태 표현
- 8/32/64/128/256 bit granularity:
  - 해당 granularity 경계에 맞는 바이트 블록을 0으로 떨어뜨리며 상태 표현

즉, status table은 “flash 재프로그래밍 제한”을 우회하기 위한 메타 프로토콜이다.

### 5.3 Rust 포팅 시 의미

Rust 구현에서는 enum 직렬화가 아니라 다음이 필요하다.
- 상태 전이 가능성 검증
- 상태 테이블 encode/decode
- write size에 맞춘 partial metadata write

이 레이어를 먼저 안정화하지 않으면 KVDB/TSDB 구현이 흔들린다.

## 6. KVDB 상세 분석

## 6.1 KVDB의 핵심 개념

KVDB는 append-only KV log에 가깝다.
하지만 단순 append-only가 아니라 sector dirty/GC/caching/recovery가 결합된 구조다.

핵심 특징:
- key는 문자열
- value는 string/blob 모두 지원
- update는 in-place overwrite가 아니라 delete + new write
- delete는 tombstone 유사 상태 전이로 처리
- sector 단위 GC 수행
- 기본값(default KV) 및 버전 기반 auto update 지원

## 6.2 KVDB 온플래시 레이아웃

### 6.2.1 Sector 헤더

`src/fdb_kvdb.c`의 `struct sector_hdr_data`

구성:
- sector store status table
- sector dirty status table
- magic = `FDB1` 계열 (`0x30424446`)
- combined
- reserved
- padding

store status:
- UNUSED
- EMPTY
- USING
- FULL

dirty status:
- UNUSED
- FALSE
- TRUE
- GC

의미:
- store status는 공간 사용 상태
- dirty status는 삭제/이동으로 인해 GC 필요 여부

### 6.2.2 KV 헤더

`struct kv_hdr_data`

구성:
- KV status table
- magic = `KV00`
- len
- crc32
- name_len
- value_len
- padding

KV 본문 배치:
- aligned header
- aligned key bytes
- aligned value bytes

CRC 범위:
- `name_len` (호환성 때문에 4바이트 취급)
- `value_len` (4바이트)
- key
- key padding(0xFF)
- value
- value padding(0xFF)

중요한 점:
CRC가 padding까지 포함된다. 따라서 Rust 재구현도 padding 바이트를 동일하게 CRC에 포함해야 포맷 호환성이 유지된다.

## 6.3 KV 상태 머신

KV 상태 enum:
- `FDB_KV_UNUSED`
- `FDB_KV_PRE_WRITE`
- `FDB_KV_WRITE`
- `FDB_KV_PRE_DELETE`
- `FDB_KV_DELETED`
- `FDB_KV_ERR_HDR`

의도된 흐름:

1. 신규 KV 생성
   - PRE_WRITE 기록
   - 헤더/키/값 기록
   - WRITE로 전이

2. 기존 KV 갱신
   - 기존 KV를 PRE_DELETE로 전이
   - 새 KV 생성 및 WRITE 완료
   - 기존 KV를 DELETED로 마무리

3. 전원 차단 등 예외
   - PRE_WRITE에서 멈춤 -> ERR_HDR로 정리 가능
   - PRE_DELETE에서 멈춤 -> recovery 시 move/recreate 대상

즉 KVDB는 “2-phase-ish update”를 사용한다.

## 6.4 KV 읽기 경로

핵심 함수:
- `read_kv`
- `find_kv`
- `get_kv`
- `fdb_kv_get_obj`
- `fdb_kv_get_blob`

`read_kv`의 핵심 동작:
- KV header raw read
- status 해석
- len 유효성 검증
- CRC 재계산
- 이름 로드
- value start address 계산

실패 처리 포인트:
- len이 비정상이면 `FDB_KV_ERR_HDR`로 상태 갱신
- CRC mismatch면 읽기 실패로 간주

즉 Rust 포팅에서는 `scan_record -> validate_header -> validate_crc -> expose view` 단계가 필요하다.

## 6.5 KV 검색과 캐시

캐시 두 종류가 존재한다.

1. KV cache
   - key name CRC16 유사 축약값 저장
   - address 캐시
   - active 카운터 기반 LRU 유사 정책

2. sector cache
   - 현재 using sector 정보 캐시
   - empty_kv 위치 및 remain 공간 캐시

핵심 포인트:
- 캐시는 correctness가 아니라 성능 최적화 계층이다.
- 캐시 miss 시 항상 flash scan으로 복구 가능하다.

Rust MVP 단계에서는 캐시를 생략해도 된다.
그러나 최종적으로 FlashDB 수준 성능을 목표로 하면 KV index cache는 다시 도입해야 한다.

## 6.6 KV 쓰기 경로

핵심 함수:
- `set_kv`
- `create_kv_blob`
- `write_kv_hdr`
- `del_kv`

쓰기 절차:

1. 새 KV가 들어갈 공간 확보
2. 필요하면 old KV를 PRE_DELETE
3. 새 KV header 작성
4. key/value aligned write
5. 새 KV를 WRITE로 전이
6. old KV를 DELETED로 완료
7. sector full이면 GC 요청 설정

중요한 세부사항:
- key/value write는 `_fdb_flash_write_align` 사용
- header status는 분리해서 먼저 기록
- sector status는 EMPTY -> USING -> FULL로 전이
- 남은 공간이 `FDB_SEC_REMAIN_THRESHOLD` 아래로 내려가면 FULL 처리

이 구조를 Rust에서 안전하게 구현하려면 `transactional append of record` 추상화가 필요하다.

## 6.7 delete와 dirty sector

`del_kv`는 KV 삭제 자체보다 “sector를 dirty로 마킹”하는 것이 중요하다.

동작:
- KV status를 PRE_DELETE 또는 DELETED로 갱신
- 해당 sector의 dirty status를 FALSE -> TRUE로 전이

의미:
- 삭제된 항목은 즉시 공간 회수되지 않는다.
- dirty sector는 나중에 GC에서 살아 있는 KV만 옮기고 sector를 erase해서 회수한다.

## 6.8 KV GC 구조

핵심 함수:
- `gc_collect`
- `gc_collect_by_free_size`
- `do_gc`
- `move_kv`

GC 알고리즘 개요:

1. empty sector 수 확인
2. threshold 이하이면 dirty sector 대상으로 GC 수행
3. GC 대상 sector를 DIRTY_GC 상태로 표시
4. sector 내 KV를 순회
5. 살아 있는 KV(`WRITE`, recovery 관점에서 `PRE_DELETE`도 포함)를 새 위치로 move
6. 원 sector format
7. `oldest_addr` 갱신

`move_kv`의 의미:
- 기존 KV를 완전히 다시 쓰는 copy-forward 방식
- 로그 구조에서 일반적인 compaction 기법

중요한 설계 특징:
- 최소 1개 empty sector를 GC 작업공간으로 남긴다.
- dirty sector 전체를 sector 단위로 수집한다.
- in-place compaction은 하지 않는다.

Rust 구현 시 GC는 반드시 별도 모듈로 분리하는 것이 좋다.

## 6.9 KV 초기화와 복구

핵심 함수:
- `fdb_kvdb_init`
- `_fdb_kv_load`
- `check_sec_hdr_cb`
- `check_and_recovery_gc_cb`
- `check_and_recovery_kv_cb`

초기화 흐름 요약:

1. 공통 init 검증 수행
2. oldest sector 계산
3. cache 초기화
4. `_fdb_kv_load` 수행
5. 필요 시 auto update 수행

`_fdb_kv_load`에서 하는 일:

1. 모든 sector header 검사
   - 손상 sector가 있으면 포맷 가능 모드에서는 자동 포맷
2. 모든 sector header가 실패하면 default KV로 초기화
3. dirty=GC sector가 있으면 GC 재개
4. 모든 KV 순회
   - PRE_DELETE인 정상 KV는 recovery move
   - PRE_WRITE는 ERR_HDR로 전이
   - WRITE는 캐시에 반영
5. recovery 중 GC 요청이 생기면 반복

즉 KVDB의 부팅 시 초기화는 사실상 “메타데이터 복구 스캔”이다.

Rust 포팅 시 이 초기 스캔은 가장 먼저 안정화해야 하는 핵심 기능이다.

## 6.10 default KV와 auto-update

관련 함수:
- `fdb_kv_set_default`
- `kv_auto_update`

기능:
- database 초기 포맷 시 기본 KV 집합 기록
- 버전 키 `__ver_num__`를 사용해 firmware upgrade 후 신규 기본값을 추가 가능

주의:
- auto update는 기존 key를 덮어쓰는 게 아니라 “없는 key만 추가”하는 방향이다.
- Rust 버전에서 이 기능을 유지할지, migration API로 분리할지는 설계 결정이 필요하다.

## 7. TSDB 상세 분석

## 7.1 TSDB의 핵심 개념

FlashDB TSDB는 key-value 형태가 아니라 time-ordered append log다.
원문에서도 TSL(Time Series Log)라는 용어를 사용한다.

특징:
- timestamp strictly increasing 요구
- append 최적화
- time-range query 지원
- reverse iteration 지원
- 각 record에 사용자 status 부여 가능
- sector rollover 지원

KVDB보다 구조는 단순하지만, sector 경계 관리와 시간 범위 탐색이 핵심이다.

## 7.2 TSDB 온플래시 레이아웃

### 7.2.1 Sector 헤더

`struct sector_hdr_data`

구성:
- sector status table
- magic = `TSL0`
- start_time
- end_info[2]
  - time
  - index
  - status table
- reserved
- padding

여기서 end_info를 2개 두는 이유는 sector 종료 메타데이터 기록도 power-loss 안전하게 만들기 위함이다.
즉 sector가 꽉 찰 때 마지막 엔트리 정보를 두 슬롯 중 하나에 2-phase로 기록한다.

### 7.2.2 TSL index + data 분리

TSDB는 sector 내부를 양쪽에서 채운다.

- 위쪽(낮은 주소)에서 index 증가
- 아래쪽(높은 주소)에서 log data 감소

즉 sector 내부 구조는 다음과 같다.

[sector header][idx1][idx2][idx3] ... free ... [data3][data2][data1]

장점:
- index scan이 빠름
- variable-size blob도 저장 가능
- data fragmentation 없이 sector 단위로 정리 가능

### 7.2.3 `struct log_idx_data`

구성:
- status table
- timestamp
- (가변 blob 모드에서) log_len, log_addr
- padding

만약 `FDB_TSDB_FIXED_BLOB_SIZE`가 활성화되면
- index에 `log_len`, `log_addr`를 저장하지 않고
- sector/slot 위치로부터 data 위치를 역산한다.

이는 flash overhead를 줄이기 위한 최적화다.

## 7.3 TSL 상태 머신

TSL 상태 enum:
- `FDB_TSL_UNUSED`
- `FDB_TSL_PRE_WRITE`
- `FDB_TSL_WRITE`
- `FDB_TSL_USER_STATUS1`
- `FDB_TSL_DELETED`
- `FDB_TSL_USER_STATUS2`

핵심 흐름:
- append 중 PRE_WRITE -> WRITE
- 이후 사용자가 status 변경 가능

KVDB와 달리 CRC 기반 payload 무결성 검증은 없다.
대신 index 상태와 sector 경계 메타데이터에 의존한다.
이 점은 Rust 포팅 시 그대로 유지할지 개선할지 검토할 가치가 있다.

## 7.4 TSDB 쓰기 경로

핵심 함수:
- `tsl_append`
- `update_sec_status`
- `write_tsl`

append 절차:

1. timestamp 검증
   - 새 timestamp는 반드시 `last_time`보다 커야 함
2. blob 길이 검증
3. 현재 sector 상태 업데이트
   - 필요 시 sector full 처리
   - end_info 기록
   - 다음 sector rollover 또는 empty sector 사용
4. index를 PRE_WRITE로 기록
5. index payload 기록
6. blob data 기록
7. index를 WRITE로 전이
8. `cur_sec.empty_idx`, `cur_sec.empty_data`, `last_time` 갱신

핵심 설계 특징:
- sector가 가득 차면 마지막 index/time을 sector header에 기록
- 이후 다음 sector를 current로 사용
- rollover=true면 ring처럼 0번 sector로 돌아갈 수 있음

## 7.5 TSDB 읽기/순회 경로

핵심 함수:
- `read_tsl`
- `fdb_tsl_iter`
- `fdb_tsl_iter_reverse`
- `fdb_tsl_iter_by_time`
- `search_start_tsl_addr`

### forward iteration
- oldest sector부터 current까지 진행
- sector 내 index를 순차 scan

### reverse iteration
- current sector의 end_idx부터 역방향 scan
- sector도 뒤로 이동

### time-range iteration
- sector의 `start_time`, `end_time`을 활용해 범위를 좁힘
- sector 내부에서는 `search_start_tsl_addr`로 index 영역 이진 탐색 비슷한 방식 사용

즉 TSDB는 전체 scan만 하는 구조가 아니라 sector-level coarse filtering + intra-sector index search를 결합한다.

## 7.6 TSDB 초기화

핵심 함수:
- `fdb_tsdb_init`
- `check_sec_hdr_cb`
- `tsl_format_all`

초기화 흐름:

1. 공통 init 검증
2. `get_time`, `max_len`, rollover 기본값 설정
3. 모든 sector header 검사
4. 이상 시 전체 포맷
5. 아니면 current sector / oldest sector 추정
6. current sector 전체 정보 로드
7. 마지막 저장 시간 `last_time` 계산

KVDB와 차이점:
- TSDB는 부팅 시 KV 수준의 복구 이동/GC를 하진 않는다.
- 대신 sector header의 일관성과 현재 sector/oldest sector 계산이 중심이다.

## 7.7 TSDB clean과 rollover

- `fdb_tsl_clean`: 전체 sector 포맷
- rollover 기본값: true
- rollover false일 때 empty sector가 더 이상 없으면 `FDB_SAVED_FULL`

Rust 포팅 시 이 옵션은 반드시 유지할 가치가 있다.
로그 시스템에서 정책 선택이 중요하기 때문이다.

## 8. 공통 초기화 및 포팅 계층 분석

## 8.1 FAL 의존성

문서 `docs/porting.md` 기준으로 FlashDB는 FAL 위에서 동작한다.
즉 FlashDB 자체는 다음 연산만 있으면 된다.

- init(optional)
- read(offset, buf, size)
- write(offset, buf, size)
- erase(offset, size)
- block/sector size
- write granularity
- partition table

Rust 포팅에서 FAL 자체를 재현할 필요는 없다.
대신 아래 trait 조합으로 충분하다.

- `embedded_storage::nor_flash::ReadNorFlash`
- `embedded_storage::nor_flash::NorFlash`
- 사용자 정의 `StorageRegion` 또는 partition wrapper

즉 FlashDB-for-rust는 “FAL replacement”가 아니라 “NorFlash region adapter”를 두는 방향이 적절하다.

## 8.2 file mode의 의미

원본은 파일 기반 백엔드를 제공한다.
이는 임베디드 본체 기능은 아니지만 다음 용도로 매우 중요하다.

- host simulation
- fast testing
- crash/reboot 재현 테스트
- CI 환경

Rust 버전도 초기 단계에서 다음 두 백엔드가 있으면 좋다.
- memory backed mock flash
- file backed sector simulator

## 9. 테스트 코드가 보여주는 설계 의도

### 9.1 KV 테스트가 보여주는 것

`tests/fdb_kvdb_tc.c`는 아래를 강하게 검증한다.

- blob/string read-write
- update/delete semantics
- GC 동작
- sector fill edge case
- file mode 기반 host 테스트
- reboot 이후 상태 지속성

즉 KVDB 구현의 품질 기준은 단순 CRUD가 아니라 “재부팅/GC 이후에도 논리 상태가 유지되는가”에 있다.

### 9.2 TSDB 테스트가 보여주는 것

`tests/fdb_tsdb_tc.c`는 아래를 검증한다.

- append
- iteration
- time-range query
- status update
- clean
- reboot 지속성
- sector/time metadata 기반 탐색

또한 timestamp가 strictly increasing해야 한다는 제약이 매우 중요하게 테스트된다.

## 10. Rust 포팅 시 그대로 가져가야 할 핵심 설계

다음은 원본과 의미적으로 최대한 동일하게 유지하는 것이 좋다.

1. sector 기반 데이터베이스 구조
2. write granularity alignment 모델
3. status table 기반 상태 전이
4. append-only update/delete 모델
5. KVDB의 sector dirty + copy-forward GC
6. TSDB의 top-index / bottom-data sector layout
7. boot-time scan and recovery
8. storage backend 추상화

특히 3, 4, 5, 6은 FlashDB의 정체성에 해당한다.

## 11. Rust 포팅 시 재설계/개선할 부분

결론부터 말하면 재설계/개선할 부분이 분명히 존재한다.
다만 전부 바꾸는 것이 아니라, FlashDB의 본질을 유지하면서 “Rust 환경에 맞게 더 안전하고 검증 가능하게” 바꾸는 쪽이 좋다.

정리하면 아래처럼 나눌 수 있다.

- 반드시 유지해야 하는 것
  - sector 기반 구조
  - write granularity 정렬
  - status table 기반 상태 전이
  - append-only 기록 + boot recovery
  - KVDB의 copy-forward GC
  - TSDB의 index/data 분리 배치
- 적극적으로 개선해도 좋은 것
  - 직렬화 방식
  - 타입 시스템
  - 동시성 모델
  - 에러 모델
  - 테스트/시뮬레이션 체계
  - TSDB 무결성 보강
  - GC/인덱싱 정책의 확장성

아래는 개선 가치가 높은 항목들이다.

### 11.1 C 구조체 직직렬화 대신 명시적 인코딩

원본은 `struct` + offset macro + raw flash write/read를 많이 사용한다.
Rust에서는 다음이 더 안전하다.

- 명시적 encode/decode 함수
- little-endian 고정 직렬화
- `#[repr(C)]` 의존 최소화

이유:
- 패딩/정렬 이슈를 제어하기 쉽다.
- 테스트 작성이 쉬워진다.
- 미래에 포맷 버전 관리가 쉬워진다.
- 필드 검증을 decode 시점에 강제할 수 있다.

권장 방향:
- `SectorHeader::encode_into(&mut [u8])`
- `SectorHeader::decode_from(&[u8]) -> Result<Self, DecodeError>`
- `KvHeader`, `TslIndex`도 동일 패턴 적용

### 11.2 상태 전이(state transition)를 타입 수준으로 제한

원본은 C라서 잘못된 상태 전이를 컴파일 단계에서 막기 어렵다.
Rust에서는 이 부분을 개선할 수 있다.

예:
- `PreWriteKv`
- `CommittedKv`
- `PreDeleteKv`
- `DeletedKv`

물론 온플래시 표현은 그대로 status table이어야 하지만, 메모리상의 내부 API는 “임의 상태 변경”보다 “합법 전이만 허용하는 메서드”가 더 안전하다.

장점:
- recovery/GC 코드에서 실수 감소
- 잘못된 순서의 write 호출 방지
- 상태 기계 테스트가 쉬워짐

### 11.3 sync API 대신 async-friendly wrapper

flash read/write/erase 자체는 Rust flash driver에 따라 blocking일 수 있고 async일 수도 있다.
초기 MVP는 blocking trait 기반으로 구현하고, 상위에서 mutex/async wrapper를 제공하는 방식이 가장 현실적이다.

권장 방향:
- 코어 DB는 storage trait 기반의 동기적 상태기계로 구현
- 상위 레이어에서 `Mutex`로 감쌈
- 필요 시 별도 `async` façade 제공

이렇게 하면 다음 장점이 있다.
- core 로직 테스트가 쉬움
- HAL 의존성 최소화
- host test와 MCU target의 동일 로직 재사용 가능

### 11.4 캐시는 MVP에서 생략 가능, 대신 플러그인 구조로 설계

초기 Rust MVP에서는 다음만 먼저 구현해도 충분하다.
- KV scan
- append
- delete
- init recovery
- GC
- TSDB append/query

그 뒤 성능 최적화 단계에서 KV cache/sector cache를 추가하는 것이 좋다.

추가 개선 포인트는 “캐시를 하드코딩하지 말고 선택적 정책으로 분리”하는 것이다.

예:
- `NoCache`
- `KvLookupCache<N>`
- `SectorMetaCache<N>`

이렇게 하면 MCU RAM 제약에 따라 쉽게 조정 가능하다.

### 11.5 TSDB payload CRC 또는 record checksum 옵션화

원본 TSDB는 KVDB처럼 payload CRC를 두지 않는다.
Rust 버전에서 포맷 호환이 목표가 아니라 “개념 재구현”이 목표라면 TSDB에도 CRC를 넣는 개선안을 검토할 수 있다.
다만 원본 의미를 최대한 존중하려면 1차 버전에서는 유지, 2차 버전에서 옵션화하는 것이 좋다.

권장안:
- v1: 원본 의미 유지, CRC 없음
- v2: feature flag 또는 format version으로 optional checksum 지원

장점:
- 로그 payload 손상 탐지 가능
- long-term logging 제품에서 신뢰성 향상

비용:
- index/header 크기 증가
- 포맷 호환성 복잡도 증가

### 11.6 timestamp 단조 증가 정책을 더 명확히 설계

원본 TSDB는 `cur_time <= last_time`이면 append를 거부한다.
이 정책은 단순하고 안전하지만, 실제 제품에서는 아래 상황이 생긴다.

- RTC 재설정
- 전원 복구 후 시간 역행
- 여러 소스에서 timestamp 입력

그래서 Rust 버전은 timestamp policy를 분리하는 것이 좋다.

예:
- `StrictMonotonic` : 원본과 동일
- `AllowEqualWithSequence`
- `ExternalTimestampWithSequence`
- `BestEffortMonotonic`

최소한 내부적으로는 timestamp가 같은 경우를 구분할 sequence 개념을 고려할 가치가 있다.
특히 TSDB query 정렬 안정성에 도움이 된다.

### 11.7 GC 정책을 “필수 동작”과 “최적화 정책”으로 분리

원본 KVDB GC는 매우 실용적이지만, 정책과 메커니즘이 같은 파일 안에 강하게 결합돼 있다.
Rust에서는 분리하는 편이 좋다.

분리 예시:
- 메커니즘
  - live record 판별
  - move/copy-forward
  - sector erase
- 정책
  - 언제 GC 시작할지
  - 얼마나 free space를 목표로 할지
  - 어떤 sector부터 수거할지

장점:
- 향후 wear-leveling 개선이 쉬움
- 테스트에서 deterministic policy 주입 가능
- 작은 flash / 큰 flash에 맞게 튜닝 가능

### 11.8 wear leveling 관점을 더 강화할 여지

원본도 sector 순환 구조로 인해 어느 정도 wear balance를 의식하고 있지만, erase count 기반의 적극적 wear leveling은 아니다.

Rust 대상 제품이 장기 운용 장비라면 개선 여지가 있다.

후보 개선안:
- sector erase count 메타데이터 추가
- GC victim 선택 시 wear count 반영
- hot key와 cold key 분리 전략
- TSDB와 KVDB의 영역 분리/고정 정책 명시

다만 이것은 MVP 범위를 넘는 고급 기능이므로 1차 구현에서는 설계 여지만 남기는 것이 적절하다.

### 11.9 오류 모델(error model) 세분화

원본은 `FDB_READ_ERR`, `FDB_WRITE_ERR`, `FDB_INIT_FAILED`, `FDB_SAVED_FULL` 정도로 단순화되어 있다.
C에서는 충분히 실용적이지만 Rust에서는 더 풍부한 오류 모델이 유용하다.

예:
- `AlignmentError`
- `OutOfBounds`
- `Storage(Read|Write|Erase)`
- `CorruptedHeader`
- `CrcMismatch`
- `NoSpace`
- `UnsupportedFormatVersion`
- `InvariantViolation`
- `TimestampNotMonotonic`

장점:
- 디버깅이 쉬워짐
- recovery 경로와 fatal error 경로를 분리 가능
- 상위 애플리케이션이 정책적으로 대응 가능

### 11.10 포맷 버전 관리 추가

원본은 일부 호환성 고려는 있지만, 온플래시 포맷 자체를 명시적으로 versioning 하는 구조는 강하지 않다.
Rust로 새로 가져갈 때는 처음부터 format version을 두는 것이 좋다.

권장안:
- sector header에 `format_version`
- major/minor 또는 단일 u16 version
- init 시 지원 가능한 버전만 mount

장점:
- TSDB checksum 추가 같은 미래 변경에 대응 가능
- 필드 추가/삭제 시 migration 전략 수립 가능

### 11.11 메모리 없는 문자열 API 제거 또는 안전화

원본 `fdb_kv_get`는 내부 버퍼 기반 string 반환이라 재진입성과 안전성 제약이 있다.
Rust에서는 이 부분을 굳이 계승할 필요가 없다.

대신 아래 중 하나가 낫다.
- caller-provided buffer에 채우기
- zero-copy view 반환(수명 제약 명확화)
- heap 없는 환경에서는 `heapless::Vec`/`heapless::String` 기반 API 제공

이건 Rust로 가면 거의 반드시 개선해야 할 부분이다.

### 11.12 no_std 친화적 테스트/시뮬레이션 구조 강화

원본은 file mode가 강력한 장점이지만, 테스트 체계가 RT-Thread 중심이다.
Rust에서는 테스트 층을 더 체계화할 수 있다.

권장 테스트 레이어:
- 단위 테스트: status table, header codec, CRC
- 모델 테스트: append/delete/recovery
- property test: 임의 입력/전원 차단 위치 fuzz
- file-backed integration test
- hardware smoke test

특히 power-loss 내성을 강조하는 라이브러리라면 “중간 단계에서 강제 리부트/재마운트” 테스트가 매우 중요하다.

### 11.13 TSDB 인덱싱 확장성 확보

원본 TSDB는 sector-level time metadata + sector 내부 검색으로 꽤 효율적이다.
하지만 데이터가 아주 커지면 추가 최적화 여지가 있다.

후속 확장 아이디어:
- sector summary table
- coarse bloom/filter metadata
- fixed-size index acceleration
- multi-reader snapshot

이건 즉시 구현 대상은 아니지만, layout과 API를 설계할 때 확장 가능성을 막지 않는 것이 좋다.

### 11.14 DB 공통부와 KV/TS 특화부의 경계를 더 명확히 분리

원본은 공통 개념이 많지만 구현은 파일 단위로 강하게 분리되어 있다.
Rust에서는 아래 경계를 더 명확히 하면 유지보수가 쉬워진다.

공통부:
- storage region
- alignment
- status codec
- flash I/O helper
- mount/scan framework 일부

KV 특화:
- record header
- key lookup
- dirty/GC semantics

TS 특화:
- sector dual-ended allocation
- timestamp query
- rollover policy

이 분리가 잘 되면 이후 `logdb`, `queue`, `event journal` 같은 파생 저장소도 만들기 쉬워진다.

### 11.15 개선 우선순위

재설계 포인트는 많지만, 우선순위는 아래처럼 잡는 것이 현실적이다.

1. 반드시 초기에 반영
   - 명시적 직렬화/디코딩
   - 에러 모델 세분화
   - safe string/blob API
   - async-friendly 구조 분리
   - format version 필드
2. 1차 MVP 이후 반영
   - 캐시 플러그인화
   - TSDB checksum 옵션
   - timestamp policy 분리
   - GC 정책 분리
3. 고급 단계에서 반영
   - wear leveling 강화
   - 고급 인덱싱
   - 다중 정책/마이그레이션 체계

즉, “재설계는 필요하지만 전면 재창조는 불필요”하다.
FlashDB의 본질은 유지하고, Rust가 잘하는 부분에서 안정성과 확장성을 끌어올리는 것이 가장 좋은 방향이다.

## 12. FlashDB-for-rust용 권장 모듈 분해안

원본 구조를 그대로 옮기기보다 아래처럼 Rust crate 구조를 나누는 것이 좋다.

### 12.1 공통 계층

- `storage/`
  - `region.rs`: flash region abstraction
  - `nor_flash.rs`: embedded-storage adapter
  - `mock.rs`: RAM/file mock backend
- `layout/`
  - alignment helper
  - status table codec
  - common constants/magic
- `crc.rs`
- `error.rs`
- `blob.rs`

### 12.2 KVDB 계층

- `kv/mod.rs`
- `kv/layout.rs`
  - sector header, record header encode/decode
- `kv/scan.rs`
- `kv/write.rs`
- `kv/gc.rs`
- `kv/recovery.rs`
- `kv/cache.rs` (후순위)

### 12.3 TSDB 계층

- `tsdb/mod.rs`
- `tsdb/layout.rs`
- `tsdb/append.rs`
- `tsdb/query.rs`
- `tsdb/recovery.rs`

### 12.4 public API 계층

- `lib.rs`
- `config.rs`
- `db.rs`

## 13. 구현 우선순위 제안

FlashDB를 Rust로 옮길 때 한 번에 KVDB+TSDB 전체를 완성하려 하면 리스크가 높다.
다음 순서를 권장한다.

### Phase 1: 공통 기반
- region abstraction
- aligned read/write helper
- status table codec
- CRC
- mock flash 테스트 환경

### Phase 2: KVDB MVP
- init/format
- append new KV
- get by scan
- delete tombstone/상태 전이
- reboot scan
- CRC recovery

### Phase 3: KVDB 고도화
- sector dirty 관리
- GC
- default KV
- iterator
- integrity check
- cache

### Phase 4: TSDB MVP
- sector layout
- append
- iter forward/reverse
- query by time
- clean

### Phase 5: TSDB 고도화
- fixed blob optimization
- status mutation
- rollover policy refinement
- crash tests

## 14. 구현 시 주의해야 할 함정

1. write granularity와 erase granularity를 혼동하면 안 된다.
2. padding 바이트도 CRC 대상에 포함되는 구간이 있다.
3. 상태 전이는 0 -> 1이 아니라 1 -> 0 단방향이어야 한다.
4. PRE_WRITE / PRE_DELETE 복구 로직을 생략하면 전원 차단 내성이 무너진다.
5. KVDB GC는 “살아 있는 레코드 이동 후 sector erase”라는 copy-forward 모델이다.
6. TSDB는 sector 내부를 양방향으로 채우므로 index/data 충돌 계산이 정확해야 한다.
7. oldest/current sector 계산 로직이 틀리면 iteration과 rollover가 모두 깨진다.
8. file-backed simulator가 없으면 회귀 테스트 속도가 매우 떨어진다.

## 15. Rust 관점에서의 직접 대응표

원본 개념 -> Rust 대응안

- FAL partition -> `StorageRegion<F: NorFlash>`
- `FDB_WRITE_GRAN` -> `F::WRITE_SIZE * 8`
- sector size -> config 또는 region metadata
- `_fdb_flash_read/write/erase` -> backend trait wrapper
- status table -> `status.rs`
- `_fdb_flash_write_align` -> aligned program helper
- `lock/unlock` callback -> `Mutex` 또는 상위 동기화 정책
- file mode -> mock/file backend for host tests

## 16. 현재 결론

FlashDB는 단순히 “플래시에 KV/TS 저장”하는 라이브러리가 아니다.
핵심은 아래 조합이다.

- flash 쓰기 제약을 반영한 상태 전이 메타데이터
- sector 단위 공간 관리
- append-only 기록
- 부팅 시 스캔/복구
- KVDB용 copy-forward GC
- TSDB용 시간축 최적화 레이아웃

따라서 Rust 버전도 단순 API 모방이 아니라, 이 불변 조건들을 재현해야 FlashDB다운 구현이 된다.

가장 먼저 안정화해야 할 것은 아래 4개다.

1. aligned flash write helper
2. status table codec
3. boot scan/recovery
4. sector layout encode/decode

이 4개가 안정되면 KVDB/TSDB는 각각 독립적인 상위 모듈로 구현 가능하다.

## 17. Blob 관점에서의 재설계/개선 여지

Blob 관점에서도 개선 여지는 분명히 있다.
오히려 FlashDB를 Rust로 가져갈 때는 KV/TS record 자체보다 Blob abstraction을 어떻게 재정의하느냐가 API 품질과 메모리 사용성에 큰 영향을 준다.

원본의 Blob은 매우 얇은 구조다.

- `buf`: 호출자가 제공한 RAM 버퍼
- `size`: 그 버퍼 크기
- `saved.meta_addr`: 메타데이터 시작 주소
- `saved.addr`: 실제 payload 시작 주소
- `saved.len`: 저장된 payload 길이

즉 원본 `fdb_blob`은
- 소유권을 갖는 데이터 타입도 아니고
- 직렬화 규약을 담는 타입도 아니며
- 사실상 “읽기/쓰기용 버퍼 descriptor + 저장 위치 메타데이터”에 가깝다.

이 설계는 C에서는 가볍고 실용적이지만, Rust에서는 몇 가지 개선 포인트가 있다.

### 17.1 Blob의 역할이 너무 넓고 동시에 너무 약하다

원본 Blob은 아래 역할을 한 구조체에 동시에 담고 있다.

1. caller buffer descriptor
2. flash에 저장된 object locator
3. read result metadata container

문제는 이 세 역할이 서로 다른 생명주기와 책임을 가진다는 점이다.

예:
- 쓰기 시에는 `buf + size`만 중요
- 읽기 전에는 `saved.*`가 의미 없을 수 있음
- `fdb_kv_to_blob`, `fdb_tsl_to_blob` 이후에는 locator 역할이 중요

Rust에서는 이를 분리하는 편이 좋다.

권장 분리안:
- `BlobRef<'a>`: 쓰기 입력용 borrowed byte slice
- `BlobLocator`: flash 안의 위치/길이/메타 위치
- `BlobReader<'a>` 또는 `BlobBuf<'a>`: 읽기 대상 버퍼

이렇게 분리하면 API 의미가 훨씬 명확해진다.

### 17.2 `void *buf` 기반 API는 타입 안정성이 약하다

원본은 `fdb_blob_make(&blob, &value, sizeof(value))` 방식이라 어떤 타입이든 bytes로 밀어 넣는다.
이건 C에서는 자연스럽지만 Rust에서는 아래 문제가 있다.

- 구조체 직렬화 규약이 불명확해질 수 있음
- endian 문제가 숨어 들어가기 쉬움
- padding byte가 우연히 저장될 수 있음
- `&T`를 그냥 bytes로 보는 습관을 유도할 수 있음

따라서 Rust에서는 Blob 계층을 “그냥 메모리 덩어리”가 아니라 “명시적으로 직렬화된 바이트열”로 다루는 편이 좋다.

권장 방향:
- low-level API는 `&[u8]`, `&mut [u8]` 기반
- high-level API는 codec trait 또는 사용자 serializer로 분리

예:
- `set_blob(key, &[u8])`
- `get_blob_into(key, &mut [u8])`
- `set_value<T: Encode>(key, &T)`
- `get_value<T: Decode>(key) -> Result<T, _>`

### 17.3 현재 Blob은 zero-copy와 streaming 모두 애매하다

원본 `fdb_blob_read`는 locator를 기반으로 caller buffer에 복사해준다.
즉 API는 항상 “읽어서 RAM에 복사”하는 모델이다.

이 구조는 단순하지만 다음 한계가 있다.

- 큰 blob을 다루기 어렵다
- 부분 읽기(offset/len)가 불편하다
- CRC/hash 검증 같은 stream 처리에 비효율적이다
- no_std RAM 제약 환경에서 부담이 생긴다

Rust 환경에서는 이 개선 가치가 높다.

권장 개선안:
- partial read API 추가
  - `read_blob_chunk(locator, offset, buf)`
- stream-like iterator 또는 reader abstraction 제공
  - `BlobCursor`
  - `BlobChunkIter`
- zero-copy에 가까운 메타 뷰 제공
  - 실제 zero-copy는 flash-mapped memory가 아니면 어렵지만, 최소한 locator 기반의 lazy read는 가능

즉 Blob은 “한 번에 전부 읽는 값”이 아니라 “flash 위 payload를 접근하는 핸들”로 보는 편이 낫다.

### 17.4 KVDB와 TSDB의 Blob 의미가 다르므로 타입을 분리할 가치가 있다

원본에서는 KV와 TSL 모두 `fdb_blob`을 재사용한다.
하지만 실제 의미는 조금 다르다.

- KV Blob
  - key에 종속된 value payload
  - update/delete semantics를 가짐
- TSDB Blob
  - append-only log payload
  - timestamp/index와 함께 해석됨

현재는 둘 다 `saved.meta_addr`, `saved.addr`, `saved.len`만 있으면 충분하지만,
Rust에서는 구분하는 편이 더 좋다.

예:
- `KvValueLocator`
- `TslPayloadLocator`

장점:
- 잘못된 API 혼용 방지
- 향후 TSDB checksum, compression, fixed-size mode 최적화 분리 쉬움
- 문서화와 테스트가 명확해짐

### 17.5 `saved.meta_addr`와 `saved.addr`를 공개 필드로 두는 대신 불변식 있는 locator 타입이 좋다

원본에서는 blob 내부에 raw address가 직접 들어간다.
이는 단순하지만 다음 문제가 있다.

- address 범위 검증 책임이 호출자/내부 코드에 분산됨
- 잘못된 locator 조합을 만들 수 있음
- meta/data 관계가 깨진 상태를 타입이 막아주지 못함

Rust에서는 다음처럼 바꾸는 편이 좋다.

- `BlobLocator { meta_addr, data_addr, len }`
- 생성자는 private
- 오직 scan/decode 함수만 생성 가능
- region 범위 검증을 constructor에서 수행

이렇게 하면 “flash에서 실제로 유효한 blob만 locator로 표현된다”는 불변식을 세울 수 있다.

### 17.6 Blob 길이와 버퍼 길이의 관계를 더 엄격히 표현할 수 있다

원본 `fdb_blob_read`는 `min(blob->size, blob->saved.len)` 만큼 읽는다.
이건 실용적이지만 호출자가 truncation을 놓치기 쉽다.

Rust에서는 이걸 더 명시적으로 만드는 게 좋다.

예:
- `read_exact_blob(locator, &mut [u8]) -> Result<(), BlobSizeMismatch>`
- `read_blob_truncated(locator, &mut [u8]) -> Result<usize, _>`
- `blob_len(locator) -> usize`

즉,
- 정확히 읽기
- 일부만 읽기
- 길이만 조회
를 분리하는 것이 좋다.

### 17.7 Blob 직렬화와 온플래시 payload 포맷을 분리해야 한다

현재 FlashDB의 blob은 사실상 “payload raw bytes”다.
이건 좋은 점도 있지만, 상위에서 구조화 데이터까지 blob으로 쓰기 시작하면 문제가 생긴다.

예:
- 센서 샘플 struct
- calibration table
- 로그 레코드
- 설정 값 묶음

이 경우 Rust에서는 Blob 계층과 codec 계층을 분리해야 한다.

권장 레이어:
- storage layer: raw blob bytes 저장/조회
- codec layer: `T <-> bytes`
- app layer: 도메인 타입

이 구분이 없으면 나중에
- endian
- versioned payload
- backward compatibility
문제가 blob API에 새어 들어온다.

### 17.8 큰 Blob 저장 시 fragment/chunk 전략을 고려할 여지가 있다

현재 KVDB는 한 KV가 사실상 하나의 연속 payload로 저장된다.
그래서 큰 blob은 다음 한계를 가진다.

- sector 공간 제약에 민감함
- 쓰기 실패 시 비용이 큼
- GC 이동 비용이 큼
- update 시 전체 재기록 필요

원본도 `sec_size`를 키워 큰 KV를 저장하는 방향이지, multi-chunk blob을 지원하진 않는다.

Rust에서 개선할 수 있는 후보:
- 대형 blob용 chunked record
- manifest + chunk list 구조
- 작은 blob과 큰 blob을 다른 경로로 저장

다만 이건 FlashDB의 단순성을 해칠 수 있으므로 다음처럼 단계화하는 게 좋다.

- MVP: contiguous blob만 지원
- 확장: large blob feature에서 chunked blob 지원

### 17.9 Blob 압축/암호화/체크섬을 payload policy로 분리 가능

원본 Blob은 순수 raw payload라 정책이 없다.
Rust에서는 payload policy를 선택적으로 얹을 수 있다.

예:
- raw
- crc-protected
- compressed
- encrypted
- compressed + crc

이걸 DB core에 박아 넣기보다는 Blob codec/policy 계층으로 두는 편이 좋다.

장점:
- core DB 단순성 유지
- 제품별 요구사항 대응 쉬움
- TSDB/KVDB에서 선택적으로 적용 가능

### 17.10 TSDB fixed-size blob 모드는 더 일반화할 수 있다

원본에는 `FDB_TSDB_FIXED_BLOB_SIZE` 최적화가 있다.
이건 좋은 아이디어지만 C 매크로 기반의 전역 정책이다.

Rust에선 더 일반화 가능하다.

예:
- `Tsdb<Fixed<32>>`
- `Tsdb<Variable>`
- 또는 runtime config의 `BlobMode::Fixed(n)` / `BlobMode::Variable`

이렇게 하면
- index 크기 최적화
- 읽기 위치 계산 단순화
- 센서 샘플 같은 고정 길이 데이터 최적화
를 더 명시적으로 활용할 수 있다.

### 17.11 Blob API는 소유권/대여 모델을 적극 활용하는 게 좋다

Rust에서 Blob 관련 가장 큰 장점은 소유권 모델을 표현할 수 있다는 점이다.

예시 방향:
- 쓰기 입력: `&[u8]`
- 읽기 버퍼: `&mut [u8]`
- flash locator: copyable small metadata type
- 필요 시 owned payload: `heapless::Vec<u8, N>` 또는 alloc feature에서 `Vec<u8>`

이렇게 하면 원본의 `fdb_blob_make` 같은 임시 descriptor API보다 사용성이 훨씬 좋아진다.

### 17.12 Blob 개선 우선순위

Blob 관점에서 우선순위를 잡으면 아래 순서가 적절하다.

1. 초기에 반드시 반영
   - `fdb_blob` 단일 구조체 역할 분리
   - `&[u8]` / `&mut [u8]` 기반 API
   - locator 타입 도입
   - exact/truncated/len query API 분리
2. 1차 MVP 이후 반영
   - partial read / cursor / chunk iterator
   - KV/TS locator 타입 분리
   - codec layer 도입
   - fixed/variable blob mode 일반화
3. 고급 단계에서 반영
   - chunked large blob
   - compression/encryption/checksum policy
   - lazy/streaming verification

### 17.13 최종 판단

Blob 관점에서 보면 원본 FlashDB의 설계는 “최소 비용의 C용 descriptor”로는 훌륭하다.
하지만 Rust에서는 그 상태로 옮기기보다 아래처럼 재정의하는 것이 더 좋다.

- Blob = raw payload 자체
- Locator = flash 상의 위치 정보
- Reader/Cursor = payload 접근 수단
- Codec = 타입 직렬화/역직렬화 정책

즉 Blob abstraction을 더 세분화하면
- RAM 사용 제어
- API 안정성
- 큰 payload 처리
- TSDB/KVDB 분리 최적화
- 향후 압축/암호화/체크섬 확장
이 모두 쉬워진다.

이 부분은 실제 구현 전에 설계 문서로 한 번 더 분리할 가치가 있다.
특히 `blob-locator`, `blob-reader`, `blob-codec` 3계층으로 나눠 정의하면 이후 구현이 훨씬 깔끔해질 가능성이 크다.

## 18. 다음 작업 제안

다음 단계로는 아래 중 하나를 권장한다.

1. FlashDB-for-rust crate의 공통 레이아웃/스토리지 추상화부터 설계
2. KVDB MVP부터 먼저 구현
3. 온플래시 포맷 명세서를 이 문서에서 별도 문서로 분리

개인적으로는 다음 순서가 가장 안전하다.

- `docs/flashdb-onflash-format.md` 작성
- `storage + layout + status` 공통 모듈 구현
- KVDB MVP 구현
- 이후 TSDB 구현

## 18. 원본 분석 시 직접 확인한 주요 참조 파일

- `~/Desktop/FlashDB/inc/flashdb.h`
- `~/Desktop/FlashDB/inc/fdb_def.h`
- `~/Desktop/FlashDB/inc/fdb_low_lvl.h`
- `~/Desktop/FlashDB/inc/fdb_cfg_template.h`
- `~/Desktop/FlashDB/src/fdb.c`
- `~/Desktop/FlashDB/src/fdb_utils.c`
- `~/Desktop/FlashDB/src/fdb_kvdb.c`
- `~/Desktop/FlashDB/src/fdb_tsdb.c`
- `~/Desktop/FlashDB/src/fdb_file.c`
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
- `~/Desktop/FlashDB/docs/porting.md`
- `~/Desktop/FlashDB/docs/api.md`

---

후속 구현 시 이 문서를 기준으로 세부 포맷 문서와 Rust 모듈 설계 문서를 추가 작성하는 것을 권장한다.
