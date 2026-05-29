# Phase 2 — 프로토콜 + 코덱: 통신 모듈의 심장 ⭐

> 예상 기간: 1.5주
> 목표: "메시지가 어떻게 구조화되고, 어떻게 바이트로 변환되는가"를 완전히 이해한다.
> **이 Phase의 과제(Put 메시지 라운드트립 추적)가 커리큘럼 전체에서 가장 중요하다.**

## 학습 목표

- zenoh 와이어 포맷 규칙(단일 바이트 / 가변 길이 정수 / 배열 / 벡터)을 안다.
- 프로토콜 메시지가 **계층별(transport / network / zenoh)** 로 나뉜 이유를 안다.
- `WCodec`(쓰기)와 `RCodec`(읽기) 트레잇으로 직렬화가 일반화된 구조를 이해한다.
- 가변 길이 정수(`zint`) 인코딩 같은 실전 기법을 읽을 수 있다.
- 하나의 메시지에 대해 **정의 → 인코딩 → 디코딩 → 테스트**를 모두 연결할 수 있다.

## 핵심 개념

zenoh는 메시지를 **세 계층**으로 나눕니다 (위에서 아래로 감쌈):

```
Transport 메시지   ← 연결 자체를 관리 (Init, Open, Frame, Close, KeepAlive ...)
   └─ Network 메시지  ← 라우팅 단위 (Push, Request, Response, Declare, Interest ...)
        └─ Zenoh 메시지  ← 애플리케이션 페이로드 (Put, Del, Query, Reply ...)
```

`Frame`(transport) 안에 여러 Network 메시지가 들어가고, Network 메시지 안에
Zenoh 메시지가 들어가는 **중첩(nesting)** 구조입니다.

**코덱**은 이 구조체들을 바이트로(직렬화), 바이트를 구조체로(역직렬화) 변환합니다.

## 읽기 로드맵

### 2-A. 프로토콜 정의 (`commons/zenoh-protocol/`)

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/lib.rs` (33~99줄) | **반드시 먼저.** 와이어 포맷 규칙: 1바이트 필드, 가변길이 정수, 배열/벡터 인코딩, 프로토콜 버전 상수 |
| 2 | `src/core/mod.rs` | 핵심 타입 모음 진입점 |
| 3 | `src/core/whatami.rs` | `WhatAmI` — 노드가 client/peer/router 중 무엇인지 |
| 4 | `src/core/locator.rs`, `endpoint.rs` | `tcp/127.0.0.1:7447` 같은 주소 표현 |
| 5 | `src/core/wire_expr.rs` | 키 표현식의 와이어 표현 |
| 6 | `src/transport/mod.rs` | 전송 메시지 종류 개요 |
| 7 | `src/transport/init.rs`, `open.rs` | **핸드셰이크 메시지** (Phase 4에서 다시 옴) |
| 8 | `src/transport/frame.rs`, `fragment.rs` | 데이터 운반 + 단편화 |
| 9 | `src/network/mod.rs` | 네트워크 메시지 종류 |
| 10 | `src/zenoh/put.rs` (또는 `src/zenoh/` 내 Put 정의) | **앱 데이터의 핵심 — Put** |

### 2-B. 코덱 (`commons/zenoh-codec/`)

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `src/lib.rs` | **`WCodec` / `RCodec` 트레잇 정의.** 코덱이 어떻게 제네릭으로 동작하는지. `Zenoh080` 등 버전 마커 |
| 2 | `src/core/zint.rs` | 가변 길이 정수(LEB128류) 인코딩 — 작은 수는 1바이트로 |
| 3 | `src/core/zslice.rs`, `zbuf.rs` | 버퍼를 와이어에 싣기 (Phase 1과 연결) |
| 4 | `src/core/zenohid.rs`, `encoding.rs`, `locator.rs` | 핵심 타입 코덱 |
| 5 | `src/transport/init.rs`, `open.rs`, `frame.rs` | 전송 메시지 코덱 |
| 6 | `src/network/` (Push 등) | 네트워크 메시지 코덱 |
| 7 | `src/zenoh/` (Put 코덱) | **Put의 인코딩/디코딩** |

> 💡 protocol과 codec은 **짝**입니다. `protocol/zenoh/put.rs`(구조)와
> `codec/zenoh/...`(직렬화)를 항상 함께 펼쳐 놓고 보세요.

## Rust 학습 포인트

- **제네릭 트레잇 디스패치**: `WCodec<Message, Writer>` / `RCodec<Message, Reader>` —
  하나의 코덱 인터페이스로 모든 메시지 타입을 다루는 설계.
- **비트 플래그 / 헤더 조작**: 메시지 헤더 한 바이트에 타입+플래그를 욱여넣는 방식.
  `<<`, `&`, `|` 비트 연산과 상수 정의.
- **`From`/`TryFrom`**: enum ↔ 바이트 변환.
- **버전 마커 타입(`Zenoh080`)**: 제로 사이즈 타입으로 프로토콜 버전을
  타입 시스템에 인코딩하는 패턴.
- **property-based 테스트**: 코덱 테스트가 임의 입력을 생성해 라운드트립을
  검증하는 방식 (`rand` 사용 지점).

## 실습 과제 ⭐ (이 커리큘럼의 핵심)

1. **연결하기**: `Put` 메시지에 대해 아래 세 가지를 모두 찾아 경로를 적으세요.
   - ① 구조체 정의: `commons/zenoh-protocol/src/zenoh/...`
   - ② 인코딩/디코딩: `commons/zenoh-codec/src/zenoh/...`
   - ③ 기존 라운드트립 테스트: 위 코덱 파일의 `#[cfg(test)]` 또는 codec의 테스트 모듈

2. **라운드트립 직접 작성**: `Put` 인스턴스를 하나 만들어
   `ZBuf`에 인코딩한 뒤 다시 디코딩해, 모든 필드가 동일한지 확인하는
   테스트를 추가하고 통과시키세요.

   ```rust
   #[test]
   fn put_roundtrip_manual() {
       let codec = Zenoh080::new();
       let original = /* Put { ... } */;
       let mut buf = ZBuf::empty();
       let mut writer = buf.writer();
       codec.write(&mut writer, &original).unwrap();
       let mut reader = buf.reader();
       let decoded: Put = codec.read(&mut reader).unwrap();
       assert_eq!(original, decoded);
   }
   ```

3. **바이트 관찰**: 인코딩된 `ZBuf`의 실제 바이트를 출력(`{:02x?}`)해보고,
   `lib.rs`의 와이어 포맷 규칙과 대조하며 "이 바이트가 무슨 필드인지" 해독.

4. **심화**: 작은 정수 `1`과 큰 정수 `100000`을 각각 `zint`로 인코딩했을 때
   바이트 수가 어떻게 다른지 확인하고 이유를 설명.

## 자가 점검 질문

- [ ] transport / network / zenoh 세 계층 메시지의 역할을 각각 한 문장으로?
- [ ] `Frame`은 무엇을 담는가? 왜 중첩 구조인가?
- [ ] 가변 길이 정수 인코딩의 장점은? 언제 손해인가?
- [ ] `WCodec`/`RCodec`를 트레잇으로 둔 덕분에 새 메시지 타입을 추가할 때 무엇이 쉬워지나?
- [ ] 메시지 헤더 한 바이트에는 보통 무엇이 들어가는가?

## 완료 체크리스트

- [ ] `protocol/src/lib.rs`의 와이어 포맷 규칙을 이해했다
- [ ] 세 계층 메시지 구조와 중첩을 설명할 수 있다
- [ ] `WCodec`/`RCodec` 트레잇 구조를 안다
- [ ] **Put의 정의→코덱→테스트를 연결**했다 (과제 1)
- [ ] **라운드트립 테스트를 직접 작성**해 통과시켰다 (과제 2)
- [ ] 인코딩된 바이트를 해독해봤다 (과제 3)

## 다음 단계

→ [Phase 3 — 링크 계층](./phase-3-links.md)
이제 이 바이트들을 실제 소켓(TCP 등)으로 어떻게 흘려보내는지 봅니다.
