# Phase 4 — 트랜스포트 계층: 신뢰성 있는 통신의 본체 ⭐

> 예상 기간: 2주 (가장 깊고 어려운 Phase)
> 목표: 연결 설정(핸드셰이크), 배치, 단편화, 우선순위 등
> "링크라는 바이트 파이프 위에 어떻게 견고한 세션을 세우는가"를 이해한다.

## 학습 목표

- `TransportManager`가 트랜스포트의 생성·관리·정리를 어떻게 총괄하는지 안다.
- 유니캐스트 **핸드셰이크(Init → Open)** 시퀀스를 양쪽(open/accept) 관점에서 그릴 수 있다.
- **배치(batch)**, **단편화/재조립(defragmentation)**, **우선순위 큐**,
  **시퀀스 번호**가 각각 어떤 문제를 푸는지 안다.
- establishment의 **확장(extension)** 메커니즘(인증, 압축, QoS 등)을 이해한다.

## 핵심 개념

```
TransportManager  (전역: 어떤 링크/트랜스포트를 만들지 정책 관리)
   │
   ├─ Unicast Transport  (피어 1:1 세션)
   │     ├─ establishment   ← Init/Open 핸드셰이크로 세션 협상
   │     ├─ link.rs          ← 이 세션이 쓰는 링크(들) 관리, multilink
   │     └─ tx/rx pipeline   ← 보낼 메시지 배치 + 받은 메시지 재조립
   │
   └─ Multicast Transport (1:N)
```

핵심 통찰: **링크는 그냥 바이트를 나르지만, 트랜스포트가 "세션"을 만든다.**
- 양쪽이 서로의 정체(ZenohId, WhatAmI)와 능력(배치 크기, 확장)을 **협상**.
- 큰 메시지를 링크 MTU에 맞게 **쪼개고**, 작은 메시지를 **묶어** 효율을 높임.
- 우선순위에 따라 전송 순서를 조정.

## 읽기 로드맵

### 4-A. 지도와 매니저

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `io/zenoh-transport/src/lib.rs` | 크레이트 전체 모듈 지도, 핵심 트레잇(`TransportPeerEventHandler` 등) |
| 2 | `io/zenoh-transport/src/manager.rs` | `TransportManager` — 설정, 링크 매니저 보유, 트랜스포트 생성 |
| 3 | `io/zenoh-transport/src/unicast/manager.rs` | 유니캐스트 전용 매니저 |
| 4 | `io/zenoh-transport/src/unicast/mod.rs` | 유니캐스트 모듈 구성 |

### 4-B. 핸드셰이크 (establishment) — 이 Phase의 하이라이트

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/unicast/establishment/mod.rs` | 핸드셰이크 단계의 공통 골격, 상태 흐름 |
| 2 | `src/unicast/establishment/open.rs` | **능동 측** — InitSyn 보내고 InitAck 받고 OpenSyn 보냄 |
| 3 | `src/unicast/establishment/accept.rs` | **수동 측** — InitSyn 받고 InitAck/OpenAck 응답 |
| 4 | `src/unicast/establishment/cookie.rs` | 쿠키 — 상태 없는 연결 검증(상태 보호) |
| 5 | `src/unicast/establishment/ext/mod.rs` | 확장 협상 골격 |
| 6 | `src/unicast/establishment/ext/auth/` | 인증 확장 (Phase 7 보안과 연결) |
| 7 | `src/unicast/establishment/ext/qos.rs`, `compression.rs` | 다른 확장 사례 |

### 4-C. 데이터 평면: 송수신 메커니즘

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/unicast/transport_unicast_inner.rs` | 유니캐스트 트랜스포트 본체 (상태, 수명주기) |
| 2 | `src/unicast/link.rs` | 세션의 링크 운영, 백그라운드 송수신 태스크 |
| 3 | `src/common/pipeline.rs` | **송신 파이프라인** — 메시지를 배치로 모아 링크로 |
| 4 | `src/common/batch.rs` | 배치 버퍼 구조, 직렬화된 메시지 누적 |
| 5 | `src/common/defragmentation.rs` | **재조립** — 쪼개진 Fragment를 원래 메시지로 |
| 6 | `src/common/seq_num.rs` | 시퀀스 번호 — 순서/유실 추적 |
| 7 | `src/common/priority.rs` | 우선순위별 큐 |
| 8 | `src/unicast/test_helpers.rs` | 테스트에서 트랜스포트를 어떻게 띄우는지 (Phase 5 연결) |

> 💡 establishment는 코드가 비대칭(open vs accept)이라 헷갈립니다.
> **두 파일을 좌우로 띄워 같은 메시지를 양쪽에서 보내고/받는 짝**으로 읽으세요.

## Rust 학습 포인트

- **상태 머신을 타입으로**: 핸드셰이크 단계가 타입/함수로 표현되어
  "잘못된 순서의 전이"가 컴파일 단계에서 막히는 설계.
- **`Arc<Mutex<...>>` / `RwLock`**: 여러 비동기 태스크가 트랜스포트 상태를 공유.
  언제 락을 잡고 언제 놓는지, 데드락 회피 패턴.
- **채널과 태스크**: 송신 파이프라인이 별도 태스크에서 도는 구조,
  백프레셔(소비자가 느리면 생산자를 막음).
- **확장 메커니즘**: 트레잇 + enum으로 선택적 기능(인증/압축/QoS)을
  플러그형으로 끼우는 설계 — 개방-폐쇄 원칙의 실제 사례.
- **`tokio::select!`**: 여러 비동기 이벤트(수신, 종료, 타임아웃)를 동시에 대기.

## 실습 과제

1. **핸드셰이크 시퀀스 다이어그램 ⭐**: open 측과 accept 측 사이에
   오가는 메시지를 순서대로 그리세요.

   ```
   Open(능동)                          Accept(수동)
      │ ── InitSyn ─────────────────────▶ │
      │ ◀───────────────── InitAck ────── │   (+cookie)
      │ ── OpenSyn ─────────────────────▶ │
      │ ◀───────────────── OpenAck ────── │
      │        세션 확립(established)       │
   ```
   각 화살표에서 **무엇이 협상되는지**(ZenohId, 배치 크기, 확장 등) 주석을 다세요.
   실제 메시지 정의는 Phase 2의 `protocol/src/transport/init.rs`, `open.rs` 참고.

2. **단편화 추적**: 링크 MTU보다 큰 메시지를 보낼 때, 어디서 `Fragment`로
   쪼개지고(`pipeline.rs`/`batch.rs`), 받는 쪽 어디서 다시 합쳐지는지
   (`defragmentation.rs`) 코드 경로를 글로 정리.

3. **배치 효과**: `batch.rs`에서 작은 메시지 여러 개가 한 배치에 모이는 조건을
   찾고, 배치가 처리량에 주는 이점을 설명.

4. **(심화) 직접 띄우기**: `test_helpers.rs`를 참고해, 두 개의 트랜스포트를
   로컬에서 핸드셰이크시키고 메시지 하나를 주고받는 작은 테스트를 작성.

## 자가 점검 질문

- [ ] 링크와 트랜스포트의 책임 차이를 한 문장으로?
- [ ] InitSyn/InitAck/OpenSyn/OpenAck 각 단계에서 무엇이 정해지는가?
- [ ] establishment에서 cookie는 어떤 공격/문제를 막는가?
- [ ] 배치와 단편화는 정반대 동작 같은데, 왜 둘 다 필요한가?
- [ ] 시퀀스 번호가 없으면 무슨 일이 생기나?
- [ ] 인증 같은 확장 기능을 핵심 핸드셰이크 코드를 고치지 않고 추가할 수 있는 이유는?

## 완료 체크리스트

- [ ] `TransportManager`의 역할을 안다
- [ ] **핸드셰이크 시퀀스 다이어그램(과제 1)** 을 그렸다
- [ ] 단편화↔재조립 경로(과제 2)를 추적했다
- [ ] 배치/우선순위/시퀀스번호가 푸는 문제를 각각 설명할 수 있다
- [ ] 확장(extension) 메커니즘을 이해했다

## 다음 단계

→ [Phase 5 — 테스트 방법론](./phase-5-testing.md) (Phase 4와 병행 권장)
방금 본 트랜스포트 코드가 어떻게 테스트되는지를 보며, 네트워크 코드 테스트법을 배웁니다.
