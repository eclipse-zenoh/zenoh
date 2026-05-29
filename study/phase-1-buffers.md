# Phase 1 — 바이트와 버퍼: 제로카피의 토대

> 예상 기간: 1주
> 목표: 네트워크 코드가 바이트를 **복사 없이** 다루는 추상화를 이해한다.
> 이 계층을 모르면 위의 코덱·트랜스포트 코드가 왜 `ZSlice`/`ZBuf`를 쓰는지 보이지 않는다.

## 학습 목표

- "왜 `Vec<u8>`을 그냥 쓰지 않는가"에 답할 수 있다 (복사 비용, 공유, `no_std`).
- `Reader` / `Writer` / `SplitBuffer` / `BackingArray` 트레잇의 역할을 구분한다.
- `ZSlice`(공유 슬라이스)와 `ZBuf`(슬라이스들의 집합)의 관계를 이해한다.
- zenoh의 에러 타입 설계(`ZResult`)를 안다.

## 핵심 개념

네트워크 라이브러리에서 데이터는 여러 계층을 오가며 잘리고(fragment) 합쳐집니다.
매번 `Vec<u8>`을 복사하면 느립니다. zenoh는 다음으로 이를 해결합니다:

- **`ZSlice`**: `Arc`로 백킹 메모리를 공유하는 불변 슬라이스. 복제(clone)해도
  데이터 복사 없이 참조 카운트만 증가.
- **`ZBuf`**: 여러 `ZSlice`를 이어붙인 논리적 버퍼. 단편화된 데이터를
  복사 없이 하나처럼 읽을 수 있음.
- **`BBuf`**: 쓰기용 가변 버퍼.
- **트레잇 추상화**: 읽기는 `Reader`, 쓰기는 `Writer`로 일반화해
  코덱이 구체 타입에 의존하지 않게 함.

## 읽기 로드맵

| 순서 | 파일 | 무엇을 볼까 |
|------|------|------------|
| 1 | `commons/zenoh-buffers/src/lib.rs` | **여기부터.** `Reader`, `Writer`, `SplitBuffer`, `BacksizedBuffer` 등 트레잇 정의와 모듈 지도. `#![no_std]` 선언 확인. |
| 2 | `commons/zenoh-buffers/src/slice.rs` | `&[u8]`에 대한 Reader/Writer 구현 — 가장 단순한 사례 |
| 3 | `commons/zenoh-buffers/src/zslice.rs` | `ZSlice` — `Arc` 기반 공유 슬라이스. clone이 왜 싼지 코드로 확인 |
| 4 | `commons/zenoh-buffers/src/zbuf.rs` | `ZBuf` — 여러 ZSlice 묶음. `Reader` 구현이 슬라이스 경계를 어떻게 넘는지 |
| 5 | `commons/zenoh-buffers/src/bbuf.rs` | `BBuf` — 쓰기 버퍼 |
| 6 | `commons/zenoh-buffers/src/vec.rs` | `Vec<u8>` 어댑터 |
| 7 | `commons/zenoh-result/src/lib.rs` | `ZResult`, `zerror!` 매크로 — 프로젝트 전역 에러 규약 |

> 💡 읽는 순서: 트레잇(추상) → 가장 단순한 구현(`slice`) → 핵심 구현(`zslice`, `zbuf`).
> 추상부터 보고 구체로 내려오면 "이 트레잇을 왜 만들었나"가 자연스럽다.

## Rust 학습 포인트

- **`no_std`**: `lib.rs`의 `#![cfg_attr(not(feature = "std"), no_std)]`.
  표준 라이브러리 없이 동작하게 만드는 임베디드 친화 설계.
- **`Arc<[u8]>`로 제로카피 공유**: `ZSlice`가 clone될 때 무슨 일이 일어나는가.
- **트레잇 기반 다형성**: 코덱이 `W: Writer`처럼 제네릭으로 받는 이유.
- **라이프타임**: 빌린 슬라이스 vs 소유한 버퍼의 경계.
- **`unsafe` 사용 지점**: 버퍼 코드에 `unsafe`가 있다면 *왜* 필요한지,
  안전 불변식(invariant)을 주석이 어떻게 보장하는지 살펴보기.

## 실습 과제

1. **기본 (라운드트립)**: `ZBuf`에 바이트 몇 개를 쓰고(`Writer`),
   다시 `Reader`로 읽어 원본과 같은지 확인하는 테스트를 작성하세요.
   `commons/zenoh-buffers/src/` 내 기존 `#[cfg(test)]`를 참고.

   ```rust
   #[test]
   fn my_roundtrip() {
       // ZBuf에 0x01, 0x02, 0x03 쓰기
       // Reader로 다시 읽어 [1,2,3]인지 assert
   }
   ```

2. **공유 확인**: `ZSlice`를 만들고 `clone()` 한 뒤, 두 핸들이 같은 백킹
   메모리를 가리키는지 확인 (포인터 비교 또는 `Arc::strong_count` 관찰).

3. **분석**: `ZBuf`의 `Reader`가 두 `ZSlice` 경계를 넘어 연속으로 읽을 때
   내부적으로 무엇을 추적하는지 코드 흐름을 글로 정리.

## 자가 점검 질문

- [ ] `ZSlice`를 clone하면 데이터가 복사되는가? 왜 그런가?
- [ ] `ZBuf`가 단일 `Vec<u8>`이 아니라 여러 슬라이스의 묶음인 이유는?
- [ ] `Reader`/`Writer`를 트레잇으로 뽑은 덕분에 코덱은 어떤 이득을 보는가?
- [ ] `no_std`가 의미하는 제약은 무엇인가?

## 완료 체크리스트

- [ ] `lib.rs`의 트레잇 정의를 이해했다
- [ ] `ZSlice`/`ZBuf`의 제로카피 메커니즘을 설명할 수 있다
- [ ] 라운드트립 테스트(과제 1)를 작성해 통과시켰다
- [ ] `ZResult` 에러 규약을 안다

## 다음 단계

→ [Phase 2 — 프로토콜 + 코덱](./phase-2-protocol-codec.md)
이제 이 버퍼 위에서 "메시지"가 어떻게 정의되고 바이트로 직렬화되는지 봅니다.
이 커리큘럼의 심장입니다.
