# Phase 7 — 심화 선택: 관심 주제 깊게 파기

> 예상 기간: 선택 (주제당 1~2주)
> 목표: Phase 0~6으로 잡은 큰 그림 위에, 관심 있는 영역 하나를 전문가 수준으로 판다.
> 또는 실제 기여(contribution)에 도전한다.

Phase 6까지 마쳤다면 zenoh 네트워크 스택의 전체 그림이 잡혀 있습니다.
이제 아래 트랙 중 끌리는 것을 고르세요. 전부 할 필요 없습니다 — **하나를 깊게**가 핵심.

---

## 트랙 A — 공유 메모리 제로카피 (zenoh-shm)

> 같은 호스트의 프로세스 간 통신에서 복사를 아예 없애는 고급 기법.

| 파일 | 무엇을 볼까 |
|------|------------|
| `commons/zenoh-shm/src/lib.rs` | SHM 모듈 지도, 핵심 타입 |
| `io/zenoh-transport/src/shm.rs`, `shm_context.rs` | 트랜스포트가 SHM을 다루는 방식 |
| `commons/zenoh-codec/src/core/shm.rs` | SHM 참조의 직렬화 |
| `examples/examples/z_pub_shm.rs`, `z_sub_shm.rs` | 사용 예제 |
| `examples/examples/z_alloc_shm.rs`, `z_posix_shm_provider.rs` | SHM 할당자 |

**학습 포인트**: 공유 메모리 세그먼트 관리, 참조 카운팅, 안전성(다른 프로세스가
메모리를 들고 있는 동안의 수명 보장), `unsafe`의 정당화.

**과제**: `z_pub_shm`/`z_sub_shm`을 돌려보고, 일반 pub/sub 대비 무엇이 복사되지
않는지 코드로 짚기.

---

## 트랙 B — 보안: 인증과 접근 제어

| 파일 | 무엇을 볼까 |
|------|------------|
| `io/zenoh-transport/src/unicast/establishment/ext/auth/` | 핸드셰이크 중 인증 협상 (Phase 4 심화) |
| `io/zenoh-transport/src/unicast/authentication.rs` | 인증 로직 |
| `zenoh/tests/authentication.rs` | 인증 시나리오 테스트 |
| `zenoh/tests/acl.rs` | 접근 제어 목록(ACL) 테스트 |
| `zenoh/src/net/routing/interceptor/` | ACL이 인터셉터로 구현되는 지점 |
| `commons/zenoh-crypto/src/lib.rs` | 암호화 유틸 |
| 링크: `io/zenoh-links/zenoh-link-tls/`, `zenoh-link-quic/` | 전송 암호화 |

**학습 포인트**: 챌린지-응답 인증, 정책을 인터셉터로 끼우는 설계,
TLS 통합.

**과제**: ACL 테스트 하나를 읽고 "허용/거부가 어느 코드에서 결정되는지" 추적.

---

## 트랙 C — QoS와 성능

| 파일 | 무엇을 볼까 |
|------|------------|
| `io/zenoh-transport/src/common/priority.rs` | 우선순위 큐 (Phase 4 복습+심화) |
| `io/zenoh-transport/src/common/pipeline.rs` | 송신 파이프라인 최적화 |
| `zenoh/tests/qos.rs`, `qos_overwrite.rs` | QoS 정책 테스트 |
| `examples/examples/z_pub_thr.rs`, `z_sub_thr.rs` | 처리량 벤치마크 |
| `examples/examples/z_ping.rs`, `z_pong.rs` | 지연 측정 |
| `commons/zenoh-stats/src/lib.rs` | 통계 수집 |

**학습 포인트**: 우선순위/신뢰성 등급, 배치 튜닝, 처리량 vs 지연 트레이드오프,
벤치마킹 방법.

**과제**: `z_pub_thr`/`z_sub_thr`로 처리량을 측정하고, 배치 크기나 우선순위
설정을 바꿔가며 수치 변화를 기록.

---

## 트랙 D — 다양한 링크 구현 비교

| 파일 | 무엇을 볼까 |
|------|------------|
| `io/zenoh-links/zenoh-link-quic/` | QUIC — 멀티스트림, 0-RTT |
| `io/zenoh-links/zenoh-link-quic_datagram/` | QUIC 데이터그램 모드 |
| `io/zenoh-links/zenoh-link-ws/` | WebSocket — 브라우저 호환 |
| `io/zenoh-links/zenoh-link-serial/` | 시리얼 — 임베디드 |
| `io/zenoh-links/zenoh-link-unixpipe/` | Unix 파이프 |
| `io/zenoh-links/zenoh-link-vsock/` | VM ↔ 호스트 |

**학습 포인트**: 각 전송 매체의 특성이 동일 트레잇 구현을 어떻게 다르게
만드는가 (Phase 3 심화).

**과제**: 가장 특이한 링크 하나(예: serial 또는 vsock)를 골라, TCP 대비
트레잇 구현이 어디서 크게 갈라지는지 분석.

---

## 트랙 E — 멀티캐스트 트랜스포트

| 파일 | 무엇을 볼까 |
|------|------------|
| `io/zenoh-transport/src/multicast/` | 1:N 트랜스포트 전체 |
| `io/zenoh-transport/src/multicast/establishment.rs` | 멀티캐스트 합류(Join) |
| `commons/zenoh-protocol/src/transport/join.rs` | Join 메시지 |

**학습 포인트**: 유니캐스트(1:1)와 다른 점 — 핸드셰이크 없이 Join,
신뢰성 보장의 한계.

**과제**: 유니캐스트 establishment(Phase 4)와 멀티캐스트 Join을 비교.

---

## 트랙 F — 실제 기여하기 (가장 추천)

학습의 완성은 기여입니다.

1. `CONTRIBUTING` 문서와 PR 규약 확인 (repo 루트 + 최근 커밋 메시지 스타일).
2. 최근 커밋/PR을 보고 코드 리뷰 문화와 테스트 요구 수준 파악.
3. 작게 시작:
   - 문서/주석 개선
   - 테스트 커버리지 추가 (Phase 5에서 배운 것 활용)
   - 작은 버그 수정 또는 예제 추가
4. 본인이 Phase 0~6에서 만든 산출물(다이어그램, 추적 문서)을 다듬어
   학습 블로그/위키로 공개 — 이해도가 한 단계 더 올라갑니다.

---

## 전체 커리큘럼 회고 (모든 트랙 공통 마무리)

아래 질문에 막힘없이 답할 수 있으면 학습 목표(아키텍처·구현·테스트·Rust)를 달성한 것입니다:

- [ ] zenoh의 6개 계층을 아래에서 위로 나열하고 각 책임을 설명할 수 있다.
- [ ] `session.put()`이 와이어 바이트가 되는 전 경로를 설명할 수 있다. (Phase 6 졸업 과제)
- [ ] 핸드셰이크가 왜 필요하고 무엇을 협상하는지 안다. (Phase 4)
- [ ] 제로카피 버퍼·코덱 트레잇·링크 트레잇·HAT 전략 등 핵심 Rust 설계 패턴을
      각각 하나씩 예로 들 수 있다.
- [ ] 단위 테스트와 통합 테스트를 직접 작성해봤다. (Phase 2, 5)
- [ ] 새 메시지 타입 / 새 링크 / 새 인터셉터를 추가하려면 어디를 건드려야 하는지 안다.

수고하셨습니다. 여기까지 왔다면 네트워크 통신 모듈을 "읽을 수 있는" 단계를 넘어
"설계 의도를 평가하고 기여할 수 있는" 단계입니다.
