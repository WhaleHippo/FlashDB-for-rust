# Testing, Validation, and Embassy Integration Plan

> 목적: FlashDB-for-embassy를 실제로 신뢰할 수 있도록 host-side 시뮬레이션, crash test, hardware smoke test, Embassy 예제까지 포함한 검증 계획을 정의한다.

## 1. 목표

이 프로젝트는 단순히 컴파일되는 것으로 충분하지 않다.
FlashDB 계열 저장소는 아래를 반드시 검증해야 한다.

- 정상 경로
- 전원 차단/중단 쓰기
- 재부팅 후 mount
- GC 이후 일관성
- time query/iteration 정확성
- 실제 보드에서의 동작 가능성

## 2. 검증 레이어

### Layer 1. pure unit tests
대상:
- alignment
- status codec
- header codec
- CRC
- blob locator/reader

### Layer 2. mock flash integration tests
대상:
- KVDB basic
- recovery
- GC
- TSDB append/query

### Layer 3. file-backed simulation
대상:
- process restart를 동반한 reboot test
- interrupted write 재현
- 장시간 시나리오

### Layer 4. hardware smoke tests
대상:
- STM32F302 + Embassy
- 실제 flash backend 위 mount/set/get/append

## 3. 예상 파일

- `tests/status_codec.rs`
- `tests/layout_kv.rs`
- `tests/layout_ts.rs`
- `tests/blob_layer.rs`
- `tests/kv_basic.rs`
- `tests/kv_recovery.rs`
- `tests/kv_gc.rs`
- `tests/ts_basic.rs`
- `tests/ts_query.rs`
- `tests/crash_scenarios.rs`
- `examples/kv_mock.rs`
- `examples/stm32f302_kv_demo.rs`
- 필요 시 `scripts/run-crash-tests.sh`

## 4. 세부 구현 단계

### Phase 1. mock NorFlash 구현 검증

목표:
- host 환경에서 가장 빠르게 반복 가능한 테스트 기반을 만든다.

mock 요구사항:
- erased state는 0xFF
- write는 1->0만 허용
- erase 후만 0xFF 복귀
- read/write/erase 범위 체크

필수 테스트:
- write 후 read
- double write 실패 또는 NOR semantics 유지
- erase 후 복원

### Phase 2. foundation unit tests 확정

목표:
- layout/status/alignment 문제를 DB 기능과 분리해서 검증한다.

반드시 테스트할 것:
- status encoding/decoding
- write-size alignment
- KV header codec
- TS header/index codec
- CRC compatibility

### Phase 3. KVDB integration tests

시나리오:
- empty mount
- set/get
- overwrite latest wins
- delete -> not found
- format -> empty
- reboot after valid write

### Phase 4. KV recovery / interrupted write tests

시나리오 예시:
- header write 후 중단
- payload 일부 write 후 중단
- final status commit 전 중단
- old record PRE_DELETE 후 중단
- GC 중 이동 중단

중요 원칙:
- “어디서 전원이 끊겼는지”를 테스트 케이스 이름에 명시한다.

### Phase 5. KV GC tests

시나리오:
- 여러 key overwrite 후 garbage 누적
- free space 부족 시 GC 발동
- live record만 이동되는지 확인
- GC 후 get 결과 동일
- GC 후 재부팅 mount 정상

### Phase 6. TSDB integration tests

시나리오:
- append only
- strict timestamp ordering
- forward iteration
- reverse iteration
- range query
- count query
- clean/reset
- rollover on/off

### Phase 7. file-backed reboot simulation

목표:
- process 간 상태 지속성과 crash 재현성을 높인다.

권장 방법:
- std feature에서 sector file simulator 제공
- 한 프로세스가 일부 write 후 종료
- 새 프로세스/새 instance가 mount

이 layer는 recovery 품질을 높이는 데 매우 중요하다.

### Phase 8. property-style 또는 fuzz-style 사고실험 테스트

가능하면 다음 종류를 도입한다.
- random key/value sequence
- random delete/update sequence
- random interrupted write point
- random reboot point

목표는 “어떤 시퀀스 후에도 mount가 깨지지 않는가”를 보는 것이다.

### Phase 9. Embassy example 작성

목표:
- 실제 사용자가 따라할 수 있는 최소 예제를 제공한다.

예상 예제:
- `examples/kv_mock.rs`
- `examples/stm32f302_kv_demo.rs`

예제 내용 권장:
- flash region 설정
- DB mount
- set/get 수행
- reboot 후 persistence 시연(가능하면 로그 또는 주석으로 설명)

### Phase 10. STM32F302 hardware smoke test

목표:
- 실제 타깃 보드에서 최소 동작을 확인한다.

권장 체크:
- cross build 성공
- 보드 flash backend 초기화 성공
- format/mount 성공
- KV set/get 1회 성공
- TS append 1회 성공(나중 단계)

### Phase 11. 문서/검증 절차 정리

목표:
- 다른 task에서도 동일하게 검증할 수 있도록 문서화한다.

권장 문서 항목:
- 어떤 test를 언제 돌릴지
- mock/file/hardware 테스트 차이
- 실패 시 먼저 봐야 할 포인트

## 5. 방법론

### 5.1 correctness -> resilience -> hardware 순서
- 먼저 정확성
- 그 다음 중단/복구 내성
- 마지막으로 실제 하드웨어 검증

### 5.2 작은 테스트를 먼저, 긴 시나리오는 나중에
- unit test가 빨라야 반복 속도가 유지된다.
- 긴 crash test는 integration layer로 분리한다.

### 5.3 각 버그를 재현 테스트로 고정
구현 중 발견한 버그는 반드시 regression test로 남긴다.

### 5.4 examples는 테스트와 별개로 유지
예제는 “사용 방법 문서”의 역할을 한다.
테스트 코드와 섞지 않는다.

## 6. 참고 자료

우선 참고:
- `docs/flashdb-architecture-analysis.md`
- 이 폴더의 모든 plan 문서

원본 참고:
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
- `~/Desktop/FlashDB/src/fdb_file.c`

Embassy/환경 참고:
- STM32F302 대상 flash backend 문서
- `embedded-storage` trait 문서
- 필요 시 probe-rs / cargo runner 설정

### 원본 FlashDB 참조 파일 정리
- `~/Desktop/FlashDB/tests/fdb_kvdb_tc.c`
  - KVDB의 정상 경로, overwrite/delete, GC, reboot 관련 시나리오를 어떤 식으로 검증하는지 참고한다.
- `~/Desktop/FlashDB/tests/fdb_tsdb_tc.c`
  - append, iter, iter_by_time, query_count, status 변경, clean 시나리오의 기준 테스트로 참고한다.
- `~/Desktop/FlashDB/src/fdb_file.c`
  - file-backed sector simulation 아이디어와 host-side persistence 테스트 구조를 설계할 때 참고한다.
- `~/Desktop/FlashDB/docs/porting.md`
  - 실제 bare-metal/flash 포팅 시 어떤 하위 정보가 필요한지 확인할 때 보조 참조한다.

## 7. 완료 기준

- foundation unit tests 통과
- KVDB integration/recovery/GC tests 통과
- TSDB append/query tests 통과
- file-backed reboot simulation 통과
- 최소 1개 Embassy 예제 제공
- STM32F302 하드웨어 smoke test 절차가 문서화됨

## 8. 후속 권장 작업

이 문서까지 구현이 어느 정도 진행되면 다음 추가 문서 작성을 고려한다.
- `docs/flashdb-onflash-format.md`
- `docs/hardware-test-procedure.md`
- `docs/regression-test-catalog.md`
