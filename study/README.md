# Zenoh로 배우는 네트워크 통신 + Rust 커리큘럼

이 폴더는 [zenoh](https://github.com/eclipse-zenoh/zenoh) 코드베이스를 교재 삼아
**네트워크 통신 모듈의 아키텍처 · 구현 · 테스트 방법 · 실전 Rust 코드**를
체계적으로 학습하기 위한 커리큘럼입니다.

## 왜 zenoh인가

- **계층이 깨끗하게 분리**되어 있어 네트워크 스택을 아래에서 위로 따라가기 좋습니다.
  (버퍼 → 프로토콜/코덱 → 링크 → 트랜스포트 → 라우팅 → API)
- `no_std` 호환, 제로카피, async/await, 트레잇 기반 다형성 등
  **실전 Rust 패턴**이 골고루 들어 있습니다.
- 테스트가 곧 "사용 예시이자 명세"라서 **테스트 작성법**을 배우기에 좋습니다.

## 학습 원칙

1. **아래에서 위로 (bottom-up)**: 바이트부터 시작해 API로 올라갑니다.
   네트워크 데이터가 흐르는 자연스러운 방향과 같습니다.
2. **각 크레이트는 `lib.rs`부터**: 모듈 트리와 최상단 주석으로 "지도"를 먼저
   그린 뒤 구현으로 들어갑니다.
3. **읽기만 하지 말고 추적**: 하나의 메시지(예: `Put`)가
   인코딩 → 전송 → 디코딩되는 전체 경로를 직접 따라갑니다.
4. **테스트를 항상 같이 읽기**: 구현 옆 `#[cfg(test)]` 모듈과 `zenoh/tests/`를
   함께 보면 "이 코드를 어떻게 쓰고 검증하는가"가 보입니다.
5. **손으로 만든다**: 각 Phase의 과제는 실제로 코드를 짜거나 테스트를 추가하는
   것으로 끝납니다. 읽기만 하면 금방 잊습니다.

## 환경 준비

```bash
# 전체 빌드 (처음엔 시간이 좀 걸립니다)
cargo build

# 예제 실행 준비 확인
cargo run --example z_sub

# 문서를 브라우저로 보기 (타입 관계 파악에 매우 유용)
cargo doc --open

# 런타임 로그로 실제 메시지 흐름 관찰
RUST_LOG=zenoh=trace cargo run --example z_sub
```

> 권장: VS Code + rust-analyzer (코드 점프 / 타입 추론 표시 필수),
> 또는 JetBrains RustRover.

## 커리큘럼 개요

| Phase | 주제 | 예상 기간 | 핵심 산출물 | 문서 |
|-------|------|----------|------------|------|
| 0 | 워밍업 — 사용자 관점 | 2~3일 | pub/sub 직접 실행 | [phase-0-warmup.md](./phase-0-warmup.md) |
| 1 | 바이트와 버퍼 | 1주 | ZBuf 라운드트립 테스트 | [phase-1-buffers.md](./phase-1-buffers.md) |
| 2 | 프로토콜 + 코덱 ⭐ | 1.5주 | Put 메시지 라운드트립 추적 | [phase-2-protocol-codec.md](./phase-2-protocol-codec.md) |
| 3 | 링크 계층 | 1주 | TCP/UDP 비교표 | [phase-3-links.md](./phase-3-links.md) |
| 4 | 트랜스포트 계층 ⭐ | 2주 | 핸드셰이크 시퀀스 다이어그램 | [phase-4-transport.md](./phase-4-transport.md) |
| 5 | 테스트 방법론 | 1주(병행) | 통합 테스트 추가 | [phase-5-testing.md](./phase-5-testing.md) |
| 6 | 라우팅 + 세션 API | 1.5주 | put 전 경로 추적 문서 | [phase-6-routing-api.md](./phase-6-routing-api.md) |
| 7 | 심화 선택 | 선택 | 관심 주제 1개 깊게 | [phase-7-advanced.md](./phase-7-advanced.md) |

⭐ = 네트워크 통신 학습의 핵심 Phase. 시간이 부족하면 0 → 2 → 4 → 6만 해도 큰 그림이 잡힙니다.

## 아키텍처 한눈에 보기

```
   애플리케이션 (z_put.rs, z_sub.rs ...)
            │
┌───────────▼────────────┐
│  API 계층               │  zenoh/src/api/      Session, Publisher, Subscriber
├────────────────────────┤
│  라우팅 / 런타임         │  zenoh/src/net/      HAT 라우팅, 디스패치, 스카우팅
├────────────────────────┤
│  트랜스포트 계층 ⭐      │  io/zenoh-transport/ 핸드셰이크, 배치, 단편화, 우선순위
├────────────────────────┤
│  링크 계층              │  io/zenoh-link*/     TCP·UDP·QUIC·TLS·WS·Serial ...
├────────────────────────┤
│  프로토콜 + 코덱 ⭐      │  commons/zenoh-protocol, zenoh-codec  메시지 정의/직렬화
├────────────────────────┤
│  버퍼                   │  commons/zenoh-buffers/  제로카피 바이트 관리
└────────────────────────┘
```

데이터는 송신 시 **위 → 아래**(API에서 만들어져 바이트로 직렬화 후 소켓으로),
수신 시 **아래 → 위**로 흐릅니다.
이 커리큘럼은 의도적으로 **아래(버퍼)에서 위(API)로** 학습합니다 —
토대를 먼저 다져야 위 계층의 설계 의도가 보이기 때문입니다.

## 진행 추적

각 Phase 문서 끝에 체크리스트가 있습니다. 완료하면 아래 표에 날짜를 적어두세요.

| Phase | 시작일 | 완료일 | 메모 |
|-------|--------|--------|------|
| 0 |  |  |  |
| 1 |  |  |  |
| 2 |  |  |  |
| 3 |  |  |  |
| 4 |  |  |  |
| 5 |  |  |  |
| 6 |  |  |  |
| 7 |  |  |  |

## 참고 링크

- 공식 사이트: https://zenoh.io
- API 문서: https://docs.rs/zenoh/latest/zenoh/
- 프로토콜 명세 주석: `commons/zenoh-protocol/src/lib.rs`
