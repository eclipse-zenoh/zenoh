# Phase 3 — 링크 계층: 전송 프로토콜 독립성

> 예상 기간: 1주
> 목표: "TCP든 UDP든 QUIC든 동일하게 다루는" 추상화를 어떻게 설계하는지 배운다.
> 좋은 트레잇 설계의 교과서적 사례다.

## 학습 목표

- 링크 추상화 트레잇(`LinkManagerUnicastTrait`, `LinkUnicastTrait`)의 책임을 안다.
- 구체 링크(TCP)가 그 트레잇을 어떻게 구현하는지 코드로 따라간다.
- "무엇이 공통(추상)이고 무엇이 프로토콜별(구체)인가"를 구분할 수 있다.
- async 소켓 프로그래밍(tokio)의 기본 패턴을 익힌다.

## 핵심 개념

상위 트랜스포트 계층은 "바이트를 보내고 받는 양방향 채널"만 필요할 뿐,
그게 TCP인지 QUIC인지 알 필요가 없습니다. 그래서 zenoh는:

- **`LinkManager*Trait`**: 링크를 **생성**(connect)하고 **수신 대기**(listen)하는 팩토리.
- **`Link*Trait`**: 만들어진 링크 하나에서 **읽기/쓰기/닫기**.
- 각 프로토콜(TCP, UDP, TLS, QUIC, WebSocket, Serial, Unix socket, vsock)은
  이 두 트레잇을 구현. 상위 계층은 `dyn` 트레잇 객체로만 다룸.

**Locator/Endpoint** (`tcp/127.0.0.1:7447`)가 어떤 링크 구현을 쓸지 결정합니다.

## 읽기 로드맵

### 3-A. 추상화 (`io/zenoh-link-commons/`)

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/lib.rs` | **여기부터.** 링크 관련 트레잇 정의와 모듈 지도 |
| 2 | `src/unicast.rs` | `LinkManagerUnicastTrait`, `LinkUnicastTrait` — 핵심 인터페이스 |
| 3 | `src/listener.rs` | 수신 리스너 관리 |
| 4 | `src/multicast.rs` | 멀티캐스트용 변형 (유니캐스트와 비교) |

### 3-B. 가장 단순한 구현부터: TCP (`io/zenoh-links/zenoh-link-tcp/`)

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/lib.rs` | 진입점, `LocatorInspector` — `tcp/...` 스킴 파싱 |
| 2 | `src/unicast.rs` | **핵심.** `TcpStream` 위에 트레잇 구현. connect/listen/read/write |
| 3 | `src/utils.rs` | 소켓 옵션(`TCP_NODELAY`, 버퍼 크기 등) 설정 |

### 3-C. 비교 학습: UDP, 그리고 라우팅 계층

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `io/zenoh-links/zenoh-link-udp/src/unicast.rs` | TCP와 무엇이 다른가 (비신뢰성, 데이터그램 경계) |
| 2 | `io/zenoh-link-commons/src/tcp.rs`, `tls.rs` | 공통 헬퍼가 어디까지 추상화됐나 |
| 3 | `io/zenoh-link/src/lib.rs` | 여러 링크 구현을 스킴별로 라우팅하는 디스패처 |

> 💡 TCP를 먼저 끝까지 본 뒤 UDP를 보면, 트레잇이 무엇을 강제하고
> 구현이 어디서 갈라지는지가 선명해집니다.

## Rust 학습 포인트

- **`async_trait`**: 트레잇 메서드를 async로 만드는 매크로. 왜 필요한지
  (네이티브 async 트레잇의 한계와 `dyn` 호환성).
- **트레잇 객체 `Box<dyn LinkUnicastTrait>`**: 런타임 다형성.
  제네릭(정적 디스패치)과의 트레이드오프.
- **tokio 소켓**: `TcpStream`, `TcpListener`, `.read()/.write_all()`.
- **`feature` 게이트**: 각 링크가 Cargo feature로 켜고 꺼지는 구조 —
  바이너리 크기/의존성 최소화.
- **에러 변환**: OS 소켓 에러를 `ZResult`로 변환하는 지점.

## 실습 과제

1. **비교표 작성**: TCP와 UDP 유니캐스트 구현을 나란히 놓고 표로 정리하세요.

   | 항목 | TCP | UDP |
   |------|-----|-----|
   | connect 방식 | | |
   | 메시지 경계 처리 | | |
   | 신뢰성 보장 주체 | | |
   | 트레잇에서 공통인 부분 | | |

2. **추적**: `tcp/127.0.0.1:7447`이라는 Locator가 주어졌을 때, 어떤 코드가
   "이건 TCP 링크다"라고 판단하고 `zenoh-link-tcp`로 연결되는지 경로를 적기.
   (힌트: `io/zenoh-link/src/lib.rs`의 스킴 매칭)

3. **분석**: `utils.rs`에서 설정하는 소켓 옵션 중 하나(예: `TCP_NODELAY`)를
   골라, 그게 켜지면/꺼지면 통신 동작이 어떻게 달라지는지 조사.

## 자가 점검 질문

- [ ] `LinkManager`와 `Link`의 책임 차이는?
- [ ] 상위 트랜스포트가 링크를 `dyn` 객체로 다루는 이점은?
- [ ] TCP는 스트림인데 zenoh의 메시지 경계는 누가 책임지는가? (→ Phase 4 복선)
- [ ] UDP 링크가 TCP보다 까다로운 점은 무엇인가?
- [ ] 새 전송 프로토콜(예: 가상의 "myproto")을 추가하려면 무엇을 구현해야 하나?

## 완료 체크리스트

- [ ] `zenoh-link-commons`의 트레잇을 이해했다
- [ ] TCP 구현을 connect→read→write→close 흐름으로 따라갔다
- [ ] TCP vs UDP 비교표(과제 1)를 작성했다
- [ ] Locator → 링크 구현 선택 경로(과제 2)를 추적했다

## 다음 단계

→ [Phase 4 — 트랜스포트 계층](./phase-4-transport.md)
링크는 "바이트 파이프"일 뿐입니다. 그 위에서 핸드셰이크·신뢰성·배치·단편화를
얹는 가장 깊은 계층으로 들어갑니다.
