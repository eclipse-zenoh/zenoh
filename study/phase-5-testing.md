# Phase 5 — 테스트 방법론: 네트워크 코드를 검증하는 법

> 예상 기간: 1주 (Phase 4와 병행 권장)
> 목표: "분산 비동기 코드를 어떻게 신뢰성 있게 테스트하는가"를 배운다.
> 학습 목표 4가지 중 하나인 **테스트 방법**을 본격적으로 다루는 Phase.

## 학습 목표

- 단위 테스트(코덱 라운드트립) ↔ 통합 테스트(세션 end-to-end)의 역할 분담을 안다.
- 두 노드를 띄워 실제로 통신시키는 **end-to-end 테스트 패턴**을 익힌다.
- `#[tokio::test]` 기반 비동기 테스트의 함정(타이밍, 직렬 실행)을 안다.
- zenoh의 테스트 헬퍼/유틸을 활용할 수 있다.

## 테스트의 3계층

| 계층 | 무엇을 검증 | 위치 예 |
|------|------------|---------|
| **단위 테스트** | 한 함수/타입의 순수 로직 (예: 코덱 라운드트립, 키 매칭) | 각 크레이트 `src/**` 내 `#[cfg(test)] mod tests` |
| **헬퍼/픽스처** | 테스트용 노드/트랜스포트를 쉽게 띄우는 도구 | `io/zenoh-transport/src/unicast/test_helpers.rs`, `commons/zenoh-test/src/lib.rs` |
| **통합 테스트** | 여러 세션이 실제로 통신하는 전체 시나리오 | `zenoh/tests/*.rs` (29개+) |

## 읽기 로드맵

### 5-A. 단위 테스트 패턴

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `commons/zenoh-codec/src/**` 의 `#[cfg(test)]` | **라운드트립 + 임의 입력 생성** 패턴 (Phase 2에서 본 것) |
| 2 | `commons/zenoh-keyexpr/src/key_expr/` 내 테스트 | 키 표현식 매칭의 경계 케이스 테스트 |
| 3 | `commons/zenoh-buffers/src/**` 의 테스트 | 버퍼 동작 단위 테스트 (Phase 1) |

### 5-B. 테스트 인프라

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `commons/zenoh-test/src/lib.rs` | 공용 테스트 유틸 — 로깅 초기화, 헬퍼 |
| 2 | `io/zenoh-transport/src/unicast/test_helpers.rs` | 트랜스포트를 테스트용으로 띄우는 빌더/팩토리 |

### 5-C. 통합 테스트 (실제 시나리오)

| 순서 | 파일 | 무엇을 검증 | 학습 포인트 |
|------|------|------------|------------|
| 1 | `zenoh/tests/session.rs` | 세션 생성/종료, 기본 pub-sub | **가장 먼저 읽기.** 두 세션 띄우는 기본 패턴 |
| 2 | `zenoh/tests/queryable.rs` | Query/Reply 왕복 | 비동기 요청-응답 검증 |
| 3 | `zenoh/tests/connectivity.rs` | 연결 상태 | 노드 발견/연결 타이밍 |
| 4 | `zenoh/tests/connection_retry.rs` | 재연결 | 실패 주입 + 복구 검증 |
| 5 | `zenoh/tests/tcp_buffers.rs` | TCP 버퍼 동작 | 링크 수준 통합 |
| 6 | `zenoh/tests/authentication.rs`, `acl.rs` | 보안 (Phase 7) | 거부/허용 시나리오 |

> 💡 통합 테스트는 "동작하는 사용 예시 모음"이기도 합니다.
> 어떤 API 사용법이 헷갈리면 `zenoh/tests/`에서 grep 하면 거의 항상 예가 나옵니다.

## Rust 학습 포인트

- **`#[tokio::test]`**: 비동기 테스트 함수를 만드는 매크로. `#[test]`와 차이.
- **`#[tokio::test(flavor = "multi_thread")]`**: 멀티스레드 런타임 옵션.
- **직렬 실행 필요성**: 네트워크 포트/리소스 충돌 때문에 일부 테스트는
  병렬로 돌리면 깨짐 → `--test-threads=1` 또는 포트 분리 전략.
- **타이밍 의존성 다루기**: 비동기 전파를 기다릴 때 고정 `sleep`이 아니라
  조건이 만족될 때까지 폴링하는 패턴 (flaky 테스트 회피).
- **테스트 격리**: 각 테스트가 자기만의 세션/포트를 쓰도록 만드는 법.

## 실습 과제

1. **읽고 해부**: `zenoh/tests/session.rs`에서 테스트 하나를 골라
   "① 노드를 어떻게 띄우고 ② 무엇을 보내고 ③ 무엇을 어떻게 단언(assert)하는지"
   3단계로 분해해 정리.

2. **통합 테스트 추가 ⭐**: 새 통합 테스트를 작성하세요.
   - 두 세션 A(구독), B(발행)를 띄운다
   - A가 `demo/study/**`를 구독
   - B가 `demo/study/test`에 알려진 페이로드를 put
   - A가 그 페이로드를 정확히 수신하는지 단언
   - (팁: 전파를 기다릴 때 무한 `sleep` 대신, 타임아웃을 건 수신 대기 사용)

3. **단위 테스트 추가**: Phase 2에서 만든 Put 라운드트립 테스트를
   "임의의 키/페이로드 여러 개"에 대해 반복 검증하도록 확장.

4. **실패 관찰**: 일부러 단언을 틀리게 만들어 테스트가 어떻게 실패 메시지를
   내는지 보고, `RUST_LOG`로 실패 시점의 로그를 함께 관찰.

## 실행 명령 모음

```bash
# 특정 크레이트 단위 테스트
cargo test -p zenoh-codec

# 특정 통합 테스트 파일
cargo test -p zenoh --test session

# 직렬 실행 (포트 충돌 회피)
cargo test -p zenoh --test session -- --test-threads=1

# 로그 보면서 (테스트는 기본적으로 출력 숨김 → --nocapture)
RUST_LOG=zenoh=debug cargo test -p zenoh --test session -- --nocapture
```

## 자가 점검 질문

- [ ] 코덱 라운드트립은 단위 테스트인가 통합 테스트인가? 왜?
- [ ] 네트워크 통합 테스트를 병렬로 돌리면 왜 깨질 수 있나? 해결책은?
- [ ] 고정 `sleep`으로 비동기 전파를 기다리는 게 왜 나쁜가?
- [ ] `--nocapture`는 언제 필요한가?
- [ ] 테스트 헬퍼(`test_helpers.rs`)가 있으면 무엇이 편해지나?

## 완료 체크리스트

- [ ] 단위 vs 통합 테스트의 역할 분담을 설명할 수 있다
- [ ] `zenoh/tests/session.rs`의 한 테스트를 3단계로 분해했다
- [ ] **통합 테스트를 직접 추가**해 통과시켰다 (과제 2)
- [ ] 비동기 테스트의 타이밍/직렬 실행 함정을 안다

## 다음 단계

→ [Phase 6 — 라우팅 + 세션 API](./phase-6-routing-api.md)
이제 모든 계층이 어떻게 하나의 시스템으로 조립되는지, 그리고 Phase 0에서 본
공개 API가 내부적으로 어떻게 구현되는지 봅니다.
