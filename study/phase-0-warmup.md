# Phase 0 — 워밍업: 사용자 관점에서 zenoh 체감하기

> 예상 기간: 2~3일
> 목표: 내부 구조를 파기 전에, **zenoh가 사용자에게 무엇을 제공하는가**를 손으로 체감한다.
> 앞으로 모든 내부 코드는 "이 사용자 경험을 어떻게 구현하는가"의 답이다.

## 학습 목표

- zenoh의 두 가지 핵심 패러다임 — **Pub/Sub**과 **Query/Reply** — 을 구분한다.
- **키 표현식(key expression)** 으로 데이터를 주소 지정하는 방식을 이해한다.
- 빌더 패턴 + `async/await` 기반의 공개 API 사용감을 익힌다.
- 두 노드를 띄워 실제로 메시지를 주고받아 본다.

## 핵심 개념 (먼저 읽기)

`zenoh/src/lib.rs`의 최상단 문서 주석(15~96줄)을 정독하세요. 여기에 다음이 설명됩니다:

- **Session**: 모든 작업의 진입점. `zenoh::open(config).await`로 생성.
- **Pub/Sub**: 발행자가 키에 값을 *put*하면, 매칭되는 구독자가 받는다 (푸시).
- **Query/Reply**: 질의자가 키 패턴으로 *get*하면, queryable이 응답한다 (풀).
- **Key expression**: `demo/example/**` 같은 계층적 + 와일드카드 주소.
- **Builder 패턴**: `session.declare_subscriber(key).callback(...).await`처럼
  체이닝으로 설정 후 `.await`로 확정.

## 읽기 + 실행 로드맵

### 1단계: 문서로 큰 그림

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `README.md` (repo 루트) | 프로젝트가 푸는 문제, 빌드 방법 |
| 2 | `zenoh/src/lib.rs` 15~96줄 | 패러다임과 핵심 타입 |
| 3 | `examples/README.md` | 어떤 예제가 있는지 |

### 2단계: 가장 단순한 예제 읽기

순서대로 읽으며 "API 호출 한 줄 한 줄이 무슨 뜻인지" 주석을 달아보세요.

| 순서 | 파일 | 패러다임 | 포인트 |
|------|------|----------|--------|
| 1 | `examples/examples/z_sub.rs` | Pub/Sub (수신) | `declare_subscriber`, 콜백 vs 채널 |
| 2 | `examples/examples/z_put.rs` | Pub/Sub (송신) | `session.put(key, value)` |
| 3 | `examples/examples/z_pub.rs` | Pub/Sub (발행자 선언) | `declare_publisher` 후 반복 put |
| 4 | `examples/examples/z_get.rs` | Query/Reply (질의) | `session.get(selector)` |
| 5 | `examples/examples/z_queryable.rs` | Query/Reply (응답) | `declare_queryable`, `query.reply(...)` |

### 3단계: 직접 실행

터미널 두 개를 띄웁니다.

```bash
# 터미널 A — 구독자
RUST_LOG=zenoh=info cargo run --example z_sub

# 터미널 B — 발행자
cargo run --example z_put
```

A에서 메시지가 찍히는지 확인하세요. 이번엔 Query/Reply:

```bash
# 터미널 A — queryable
cargo run --example z_queryable

# 터미널 B — 질의
cargo run --example z_get
```

> 💡 두 프로세스가 서로를 어떻게 찾았을까요? (힌트: 멀티캐스트 스카우팅 —
> Phase 6에서 다룹니다. 지금은 "자동으로 발견된다"만 알면 됩니다.)

## Rust 학습 포인트

- **빌더 패턴 + `IntoFuture`**: `declare_subscriber(...)`가 즉시 실행되지 않고
  `.await` 시점에 확정되는 구조. 왜 이렇게 설계했을까?
- **클로저 콜백**: `.callback(|sample| { ... })`에서 `move`와 캡처.
- **`Result`와 `?`**: 예제들이 에러를 어떻게 전파하는지.
- **`async fn main`과 tokio**: `#[tokio::main]` 매크로의 역할.

## 실습 과제

1. **기본**: `z_sub.rs`를 복사해 수정 — 받은 샘플의 **키, 페이로드 바이트 길이,
   (있다면) 타임스탬프**를 한 줄로 출력하도록 만들기.
2. **응용**: `z_put.rs`가 보내는 키를 `demo/example/study`로 바꾸고,
   구독자의 키 표현식을 `demo/example/**`로 두어 와일드카드 매칭을 확인.
3. **관찰**: `RUST_LOG=zenoh=debug`로 구독자를 띄우고, 발행자를 켰을 때
   로그에 무엇이 찍히는지 살펴보기 (아직 이해 못 해도 OK — 나중에 다시 옵니다).

## 자가 점검 질문

- [ ] Pub/Sub과 Query/Reply의 차이를 한 문장으로 설명할 수 있다.
- [ ] 키 표현식에서 `*`와 `**`의 차이는?
- [ ] `session.put()`과 `declare_publisher().put()`은 언제 각각 쓰나?
- [ ] 콜백 방식과 채널(handler) 방식 구독의 장단점은?

## 완료 체크리스트

- [ ] `zenoh/src/lib.rs` 상단 문서를 읽었다
- [ ] 5개 예제를 읽고 주석을 달았다
- [ ] pub/sub과 query/reply를 각각 직접 실행해 메시지를 주고받았다
- [ ] 과제 1, 2를 완료했다

## 다음 단계

→ [Phase 1 — 바이트와 버퍼](./phase-1-buffers.md)
이제 "값(value)"이 내부에서 어떤 바이트 구조로 다뤄지는지 가장 아래 계층부터 봅니다.
