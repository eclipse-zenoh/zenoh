# Phase 6 — 라우팅 + 세션 API: 모든 계층의 조립

> 예상 기간: 1.5주
> 목표: 트랜스포트 위에서 "메시지를 누구에게 보낼지" 결정하는 라우팅과,
> Phase 0에서 본 공개 API가 내부적으로 어떻게 동작하는지 이해한다.
> 여기서 커리큘럼 전체가 하나로 연결된다.

## 학습 목표

- 세션이 `open`될 때 런타임·트랜스포트·라우팅이 어떻게 초기화되는지 안다.
- **스카우팅(scouting)** 으로 노드가 서로를 자동 발견하는 방식을 안다.
- **HAT(Hybrid Architecture Tables)** — client/peer/router 모드별 라우팅 전략을 구분한다.
- 키 표현식 기반으로 구독자/queryable을 찾아 메시지를 디스패치하는 흐름을 안다.
- `session.put()` 한 줄이 와이어 바이트가 되기까지의 **전 경로**를 추적할 수 있다.

## 핵심 개념

```
zenoh::open(config)
   │
   ▼
Runtime  ── Orchestrator ── Scouting(멀티캐스트로 피어 발견)
   │            │
   │            └─ TransportManager (Phase 4) ── Links (Phase 3)
   │
   ▼
Routing
   ├─ HAT (모드별 라우팅 테이블: client / peer / router / broker)
   ├─ Dispatcher (들어온 메시지를 매칭되는 로컬/원격 대상으로)
   ├─ Interceptor (메시지 가로채기: 로깅, ACL, QoS 덮어쓰기 ...)
   └─ Namespace / Gateway

API 계층 (Phase 0에서 본 것)
   Session ── Publisher / Subscriber / Querier / Queryable
```

핵심 통찰: **키 표현식이 곧 라우팅 주소**입니다. 구독자는 키 패턴을 선언해
"테이블"에 등록되고, put이 들어오면 디스패처가 매칭되는 대상을 찾아 전달합니다.

## 읽기 로드맵

### 6-A. 세션과 런타임 초기화

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `zenoh/src/api/session.rs` | `Session` 생명주기, `open`/`close`, 선언(declare) 진입점 |
| 2 | `zenoh/src/net/runtime/mod.rs` | 런타임 초기화 — 트랜스포트·라우팅을 묶음 |
| 3 | `zenoh/src/net/runtime/orchestrator.rs` | **스카우팅** — 멀티캐스트로 피어 발견, 연결 수립 |
| 4 | `zenoh/src/net/runtime/adminspace.rs` | 관리용 네임스페이스(`@/...`) |

### 6-B. 라우팅 엔진

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `zenoh/src/net/routing/mod.rs` | 라우팅 모듈 지도 |
| 2 | `zenoh/src/net/routing/dispatcher/` | 들어온 메시지를 대상으로 분배하는 핵심 로직 |
| 3 | `zenoh/src/net/routing/hat/mod.rs` | HAT 추상화 — 모드별 라우팅 전략 트레잇 |
| 4 | `zenoh/src/net/routing/hat/client/` | 가장 단순한 모드(클라이언트)부터 |
| 5 | `zenoh/src/net/routing/hat/peer/`, `router/` | 더 복잡한 토폴로지 |
| 6 | `zenoh/src/net/routing/interceptor/` | 인터셉터 체인 (ACL·QoS 등을 끼우는 지점) |
| 7 | `zenoh/src/net/primitives/` | 라우팅 프리미티브 — 상하 계층의 접합부 |

### 6-C. 공개 API 구현 (Phase 0과 연결)

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `zenoh/src/api/publisher.rs` | `put`이 내부에서 무엇을 호출하는지 |
| 2 | `zenoh/src/api/subscriber.rs` | 구독 선언이 라우팅 테이블에 어떻게 등록되는지 |
| 3 | `zenoh/src/api/query.rs`, `queryable.rs` | Query/Reply의 내부 |
| 4 | `zenoh/src/api/handlers/` | 콜백 vs 채널(FIFO 등) — 수신 메시지를 사용자에게 전달하는 방식 |
| 5 | `commons/zenoh-keyexpr/src/key_expr/`, `keyexpr_tree/` | **키 매칭과 트리** — 라우팅의 자료구조 |

> 💡 `keyexpr_tree`는 "어떤 구독자가 이 키에 매칭되는가"를 빠르게 찾기 위한
> 핵심 자료구조입니다. 라우팅 성능의 비밀이 여기 있습니다.

## Rust 학습 포인트

- **대규모 모듈 조직**: 50K LOC가 어떻게 `api`/`net`으로 나뉘고
  공개/비공개 경계(`pub`, `pub(crate)`)가 그어졌는지.
- **트레잇으로 전략 교체(HAT)**: 같은 인터페이스에 client/peer/router 구현을
  꽂아 넣는 전략 패턴.
- **빌더 + `IntoFuture`**: `declare_subscriber(...).res().await` 류 API의 구현부.
- **콜백 vs 채널**: `handlers/`에서 동기 클로저와 비동기 채널을 통일된
  인터페이스로 다루는 설계.
- **인터셉터 체인**: 미들웨어 패턴을 Rust 트레잇으로.
- **트리 자료구조 + 라이프타임/Arc**: keyexpr_tree의 공유·동시성 처리.

## 실습 과제 ⭐ (졸업 과제)

1. **put 전 경로 추적 ⭐**: `examples/examples/z_put.rs`의 `session.put(...)`
   한 줄에서 출발해, 데이터가 와이어 바이트가 되기까지 거치는 코드를
   계층 순서대로 따라가 문서로 정리하세요. 거치는 지점(대략):

   ```
   api/session.rs (put)
     → api/publisher.rs (PublicationBuilder)
       → net/primitives (push)
         → net/routing/dispatcher (매칭 대상 찾기, keyexpr_tree)
           → 로컬 구독자 콜백  AND/OR  원격 전송
             → io/zenoh-transport (Frame에 담아 배치)   [Phase 4]
               → codec (직렬화)                          [Phase 2]
                 → io/zenoh-link (소켓 write)             [Phase 3]
   ```
   각 화살표에서 호출되는 실제 함수명을 채워 넣으세요.
   (rust-analyzer "Go to definition" + `RUST_LOG=zenoh=trace` 로그 병행 추천)

2. **스카우팅 관찰**: 두 노드를 띄우고 `RUST_LOG=zenoh=debug`로
   서로를 발견·연결하는 로그를 캡처해, `orchestrator.rs` 코드와 대조.

3. **키 매칭 실험**: `keyexpr` 단위 테스트를 보고, `demo/*/x`가
   `demo/a/x`에는 매칭되고 `demo/a/b/x`에는 매칭 안 되는 이유를 코드로 설명.

4. **(심화) 인터셉터 한 줄 추가**: 인터셉터 체인을 읽고, 지나가는 메시지의
   키를 로그로 찍는 최소 인터셉터가 어디에 어떻게 끼는지 추적
   (실제 구현까진 안 해도, "끼우는 지점"을 특정).

## 자가 점검 질문

- [ ] `zenoh::open`부터 통신 준비 완료까지 무슨 일이 순서대로 일어나는가?
- [ ] 노드들은 설정 없이도 어떻게 서로를 찾는가?
- [ ] client / peer / router 모드의 라우팅 차이를 한 문장씩?
- [ ] 구독자를 선언하면 라우팅 입장에서 무슨 일이 생기는가?
- [ ] put 메시지가 매칭 대상을 찾는 데 쓰는 자료구조는?
- [ ] 인터셉터는 어떤 기능들을 "핵심 코드 수정 없이" 가능하게 하나?

## 완료 체크리스트

- [ ] 세션 초기화 흐름을 설명할 수 있다
- [ ] HAT 모드별 차이를 안다
- [ ] **put → 와이어 전 경로(과제 1)** 를 추적해 문서화했다 ← 졸업 과제
- [ ] 키 매칭/트리 자료구조의 역할을 안다
- [ ] 콜백 vs 채널 핸들러 차이를 안다

## 다음 단계

→ [Phase 7 — 심화 선택](./phase-7-advanced.md)
큰 그림이 완성됐습니다. 이제 관심 있는 주제 하나를 깊게 파거나,
실제 기여(contribution)에 도전합니다.
