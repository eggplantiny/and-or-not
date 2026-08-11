# A/O/N — Simulation Semantics Specification

**Full Title:** AND OR NOT — Simulation Semantics Specification
**Short Title:** A/O/N SSS
**Version:** v1.0 Draft
**Companion Product Contract:** A/O/N PRD v1.0 — GO
**Normative Scope:** Stage 0 Emergence Probe / Stage 1 Capacity Economy Probe / Stage 2 Relay Expansion Probe / Emergent Defense MVP
**Status:** Stage 0 implementation contract. Stage 1·2 semantic laws included; gameplay coefficients remain versioned profile values.

---

## v1.0 재작성 요약

이 문서는 SSS v0.2를 PRD v1.0에 맞춰 다시 작성한 정본 후보이다.

v0.2에서 유지한다.

- 고정 Simulation Tick
- `LOW / HIGH / X`
- Gate Inertial Delay
- Wire Transport Delay
- 실제 Wire Length와 Fan-out
- Feedback / Startup / Glitch / Hazard
- Signal / Power / Sense / Track을 공유하는 하나의 Wire Body
- Power Region과 Brownout
- Heat / Electrical·Thermal Damage
- Wire-guided Mobile Substrate
- Switching Energy 기반 결정론적 Radiation
- Module flatten
- Host 독립 결정론

v1.0에서 추가하거나 명확히 한다.

- Main Core와 Relay Site의 Canonical 상태
- Global Network Capacity의 정확한 단위와 Wire Length accounting
- Multi-role Wire를 한 번만 계산하는 규칙
- Capacity 초과를 Build 금지가 아닌 Support Overhead로 변환하는 Soft Limit
- Relay Online / Offline / Destruction / Reconstruction 시맨틱스
- Physical Scale Profile과 Circuit/World scale의 버전 계약
- Fixed-point scale, 정수 나눗셈, Euclidean length, 음수 좌표 floor 규칙
- Topology 변경 시 기존 Driver Sample 동기화
- Same-tick Player Command ordering
- In-flight Route identity와 EntityId 재사용 금지
- Laboratory live-edit / reset 계약
- Stage별 Conformance Gate

핵심 변화는 다음 한 문장으로 요약한다.

> **Network Capacity는 Wire를 금지하는 별도 규칙이 아니라, 실제 Wire Length가 기존 Power와 Heat 시스템에 추가 유지 부담을 만드는 전역 지원 한계다.**

---

# 0. 문서의 목적과 권위

이 문서는 A/O/N 세계에서 플레이어가 관찰할 수 있는 상태와 상태 전이의 정본(canonical contract)을 정의한다.

문서 간 책임은 다음과 같다.

```text
제품 목표·가설·범위·비목표
→ PRD

관찰 가능한 World / Circuit 동작
→ Simulation Semantics Specification

자료구조·알고리즘·패키지·캐시·렌더링·성능 최적화
→ TRD
```

충돌 시:

- 제품이 무엇을 검증하는지는 PRD를 따른다.
- 같은 Layout과 Input이 실제로 어떤 결과를 만드는지는 이 문서를 따른다.
- 같은 결과를 어떤 기술로 계산하는지는 TRD가 결정한다.

엔진 최적화는 다음을 바꿀 수 없다.

- Signal Waveform
- Gate / Wire Delay
- Glitch / Hazard
- Network Capacity Usage
- Overcapacity Support Demand
- Relay Online 상태
- Power / Brownout
- Heat
- Movement
- Radiation Arrival
- Damage / Destruction
- Replay State Hash

이 문서에서:

- **MUST**: 반드시 지켜야 한다.
- **MUST NOT**: 절대 해서는 안 된다.
- **SHOULD**: 명시적 근거가 없다면 지켜야 한다.
- **MAY**: 구현 선택이 가능하지만 Canonical 결과는 같아야 한다.

---

# 1. Stage Readiness

## 1.1 Stage 0

Stage 0 구현에 필요한 시맨틱스는 이 문서에서 모두 닫힌다.

```text
A/O/N
Wire
Junction
Mobile Substrate
Delay
Feedback
Startup
Waveform
Laboratory
Determinism
```

Stage 0은 Network Capacity, Relay, Radiation, Payload, Repair를 구현하지 않아도 된다.

## 1.2 Stage 1

Stage 1의 다음 법칙도 이 문서에서 정의한다.

```text
Main Core Capacity
Global Network Capacity
Wire Length Accounting
Soft Overcapacity
Sensing
Power / Brownout
Contact Attack
Construction Work
```

다만 Stage 1의 Crossover를 결정하는 숫자는 단일 고정값이 아니라 versioned `PhysicalScaleProfile`과 `BalanceProfile`의 Parameter Sweep으로 검증한다.

## 1.3 Stage 2

Stage 2의 Relay 상태 전이와 Capacity 기여 규칙을 정의한다.

Relay 한 기의 Capacity Amount, Activation Work, Upkeep는 Balance Parameter다.

## 1.4 MVP

Radiation, Heat, Damage, Payload, Reconstruction, Quartz는 이 문서의 시맨틱스를 따른다.

---

# 2. 절대적 Simulation 불변식

1. 계산 Primitive는 `AND / OR / NOT`뿐이다.
2. Wire는 하나의 Physical Body다.
3. Wire Body는 Signal / Power / Sense / Track 표면을 공유한다.
4. 모든 Circuit은 실제 Gate와 Wire Geometry로 존재한다.
5. Circuit 내부 Wire도 World Wire와 같은 Length·Capacity·Delay 법칙을 따른다.
6. Module은 Runtime Black Box가 아니다.
7. Signal은 시간에 따른 Driver Sample이다.
8. 모든 인과 경로는 1 Tick 이상의 양의 지연을 가진다.
9. Feedback은 같은 Tick의 fixed-point solve가 아니라 시간에 따라 전개된다.
10. Geometry와 Timing은 Energy를 재분배할 수 있지만 생성할 수 없다.
11. Network Capacity는 Wire Build Permission이 아니다.
12. Overcapacity는 Wire 삭제나 즉시 Failure를 일으키지 않는다.
13. Overcapacity의 영향은 Power Demand와 Heat를 통해 나타난다.
14. Relay는 계산·라우팅·버퍼링을 수행하지 않는다.
15. Damage Type은 Electrical / Thermal뿐이다.
16. 같은 Initial State와 Input Log는 Host 성능과 무관하게 같은 결과를 만든다.
17. Runtime EntityId는 한 Run 안에서 재사용하지 않는다.
18. 플레이어가 관찰할 수 있는 모든 계산은 정수 또는 고정소수점으로 재현 가능해야 한다.

---

# 3. Versioned Contract와 Profile

A/O/N의 한 Run은 다음 네 계약을 명시한다.

```ts
type SimulationContract = {
  semanticsVersion: string;
  numericProfileHash: string;
  physicalScaleProfileHash: string;
  balanceProfileHash: string;
};
```

## 3.1 Semantics Version

다음이 바뀌면 `semanticsVersion`을 올린다.

- Tick Phase Ordering
- Truth Table
- Gate Inertial / Wire Transport
- Wire Length Accounting 방식
- Overcapacity 함수의 형태
- Relay 상태 전이
- Damage Commit 순서
- Radiation allocation 방식
- Deterministic Tie-break

## 3.2 Numeric Profile

다음이 바뀌면 `numericProfileHash`를 바꾼다.

- Fixed-point scale
- 나눗셈 rounding
- Euclidean length rounding
- coordinate floor
- overflow policy

## 3.3 Physical Scale Profile

다음이 바뀌면 `physicalScaleProfileHash`를 바꾼다.

- Gate Minimum Footprint
- Gate Port 위치
- Circuit Routing Pitch
- World Routing Pitch
- Wire Geometry Quantum
- Wire Body Radius
- Substrate routing area

## 3.4 Balance Profile

다음은 `balanceProfileHash`에 포함한다.

- Gate / Wire Delay 계수
- Fan-out 계수
- Power와 Heat 계수
- Main Core Capacity
- Relay Capacity
- Overcapacity curve 계수
- Construction Work
- Movement Speed
- Sensing Radius
- Radiation Kernel
- Damage Tolerance
- Quartz Period

Formula의 형태가 같고 계수만 바뀌면 Semantics Version은 유지할 수 있다.

## 3.5 Module Compatibility

Module Blueprint는 다음을 저장해야 한다.

```text
semanticsVersion
numericProfileHash
physicalScaleProfileHash
absolute fixed-point geometry
```

다른 Physical Scale Profile에서 Module을 불러올 때 엔진은 암묵적으로 크기를 재조정해서는 안 된다.

가능한 처분은 둘뿐이다.

- 정확히 호환되어 그대로 사용
- 명시적인 migration 결과로 새 Module 생성

---

# 4. Canonical Numeric Domain

## 4.1 기본 타입

```ts
type Tick = bigint;
type EntityId = bigint;
type Revision = bigint;

type Fixed = bigint;
type Energy = bigint;
type HeatEnergy = bigint;
type Integrity = bigint;
type DriveStrength = bigint;
type Capacity = bigint;
```

Canonical Core는 `f32` 또는 `f64` 결과에 의존해서는 안 된다.

렌더러는 보간에 float를 사용할 수 있지만 Canonical State Hash에는 포함하지 않는다.

## 4.2 Numeric Profile v1

```text
FIXED_ONE = 65,536
1 world unit = 65,536 Fixed
```

좌표와 길이는 signed 64-bit 범위를 사용할 수 있다. 중간 곱셈과 제곱은 최소 signed/unsigned 128-bit를 사용한다.

## 4.3 Canonical Division

`d > 0`일 때 다음 helper를 정본으로 사용한다.

```text
floor_div(n, d)
ceil_div_nonnegative(n, d)
round_div_nearest_even(n, d)
```

- Cell 좌표 변환은 `floor_div`를 사용한다.
- Delay, Work, Capacity Demand처럼 0으로 사라지면 안 되는 양은 `ceil_div_nonnegative`를 사용한다.
- 일반 fixed-point coefficient 곱은 `round_div_nearest_even`을 사용한다.
- 각 공식이 별도 rounding을 명시하면 그 규칙이 우선한다.

음수 정수 나눗셈을 언어 기본 truncation에 맡겨서는 안 된다.

## 4.4 Geometry Quantization

모든 배치 좌표는 `wireGeometryQuantum`의 정수배여야 한다.

Reference Physical Scale Profile:

```text
wireGeometryQuantum = 1 / 64 world unit
```

Host가 더 미세한 위치를 입력하면 Phase 0 Command Validation에서 거부한다. 자동 반올림하지 않는다.

## 4.5 Euclidean Segment Length

Polyline Segment의 canonical length:

```text
dx = x2 - x1
dy = y2 - y1
segmentLength = ceil_isqrt(dx² + dy²)
```

`ceil_isqrt`는 결과를 Fixed unit으로 반환한다.

Wire Length:

```text
consecutive same-direction collinear segments
→ maximal collinear run

wireLength = Σ segmentLength(maximal run)
```

이 canonicalization은 Length 계산에만 적용한다. 저장된 vertex와 State Hash는 바꾸지 않는다.
따라서 불필요한 collinear vertex가 segment별 ceiling을 중복 발생시키지 않는다.

이 규칙은 다음에 공통으로 사용한다.

- Network Capacity Usage
- Signal Path Length
- Power Path Length
- Construction Work
- Physical Exposure

## 4.6 Cell Coordinate

```text
cellX = floor_div(worldX, cellSize)
cellY = floor_div(worldY, cellSize)
```

음수 좌표에서도 수학적 floor를 사용한다.

## 4.7 Overflow

Canonical arithmetic는 wrapping을 사용하지 않는다.

```text
Overflow
→ deterministic NumericOverflow
→ Run / Replay 중단
```

Saturating arithmetic는 이 문서나 Balance Profile이 명시한 Energy accumulator에만 사용할 수 있다.

---

# 5. Canonical Identity와 World State

## 5.1 EntityId

EntityId는 한 Run에서 단조 증가하며 재사용하지 않는다.

파괴된 Wire와 같은 Geometry에 Wire를 다시 만들더라도 새 EntityId를 받는다.

이 규칙은 다음의 정본이다.

- Event path validity
- Replay trace
- Tie-break
- Reconstruction provenance

## 5.2 최소 World State

```ts
type WorldState = {
  tick: Tick;
  topologyRevision: Revision;

  mainCore: MainCoreState;
  relaySites: RelaySiteState[];

  gates: GateState[];
  wires: WireSegmentState[];
  junctions: JunctionState[];
  fixedSubstrates: FixedSubstrateState[];
  mobileSubstrates: MobileSubstrateState[];

  powerSources: PowerSourceState[];
  quartzNodes: QuartzState[];
  deposits: DepositState[];
  enemies: EnemyState[];

  constructionSites: ConstructionSiteState[];
  thermalCells: ThermalCellState[];

  pendingDriverTransitions: DriverTransition[];
  pendingSignalArrivals: SignalArrival[];
  pendingRadiationArrivals: RadiationArrival[];
  pendingDestructions: EntityId[];
  pendingRelayTransitions: RelayTransition[];

  pathCertificates: PathCertificate[];

  contract: SimulationContract;
};
```

Network Usage, Supported Capacity, Overcapacity Ratio는 현재 Active Entity에서 매 Tick 재계산 가능한 Derived State다.

Replay와 Analyzer는 해당 값을 기록할 수 있으나 별도의 독립 Truth로 저장해서는 안 된다.

## 5.3 Module

Module은 Canonical Runtime Entity가 아니다.

Module Placement는 실제 Gate / Wire / Junction / Substrate Construction Site 집합을 생성한다.

Module 계층은 UI, 편집, provenance 메타데이터다.

---

# 6. 시간 모델

## 6.1 고정 Tick

```text
t = 0, 1, 2, 3, ...
```

Wall Clock과 Simulation Tick은 분리한다.

Host가 느려져도 다음을 해서는 안 된다.

- Tick skip
- Event drop
- 평균값 근사
- 거리 기반 저정밀 Simulation
- FPS에 따른 Delay 변경

`Simulation::step()` 한 번은 정확히 한 Canonical Tick이다.

## 6.2 양의 지연

Gate Delay와 Wire Delay의 최솟값은 1 Tick이다.

Radiation Propagation Delay도 최소 1 Tick이다.

Zero-delay Feedback Loop는 존재하지 않는다.

---

# 7. Tick 내 Event Ordering

Tick `t`는 다음 12 Phase로 수행한다.

## Phase 0 — Structural Commit

다음 순서로 Structural State를 변경한다.

1. 이전 Tick의 pending destruction 적용
2. Reconstruction Site 생성
3. pending Relay Online / Offline transition 적용
4. 완료된 Construction Primitive 활성화
5. Tick `t`의 Player Command를 ordinal 순서로 적용
6. Module Placement를 primitive Construction Site로 flatten
7. topology revision 증가 여부 확정
8. Signal / Power / Track topology 재구성
9. 끊어진 route의 sink slot 제거
10. 새 route에 Topology Synchronization Arrival 예약

Relay 파괴로 Capacity가 감소하는 시점은 이 Phase다.

Wire가 활성화되어 Network Usage에 포함되는 시점도 이 Phase다.

## Phase 1 — Snapshot & World Sample

Tick 시작 상태를 immutable snapshot으로 고정한다.

샘플 대상:

- Entity position
- Integrity
- Temperature
- Relay mode
- Online Capacity
- Hostile Occupancy
- Quartz phase
- Enemy deterministic intent
- Player actuator input
- Topology connectivity

Tick 중간 이동은 다음 Tick Sensing에 반영된다.

## Phase 2 — Driver / Signal Arrival

1. 현재 Tick에 due인 Driver Transition을 모두 적용한다.
2. 실제 Driver Sample이 바뀐 Driver의 새로운 Revision을 생성한다.
3. 해당 Revision을 current topology route로 미래 Signal Arrival에 예약한다.
4. 현재 Tick에 due인 Signal Arrival을 모두 적용한다.
5. Sink별 Driver Slot을 한 번 resolve한다.

동일 Tick 배열 순서는 결과에 영향을 주지 않는다.

## Phase 3 — Intent Evaluation

현재 resolved Signal을 기준으로 의도를 계산한다.

- Gate desired output
- Sensor sample
- Quartz output sample
- Wire live-drive intent
- Mobile control
- LOAD / UNLOAD / BUILD
- Extraction
- Relay activation / upkeep intent
- Radiation emission intent
- Enemy attack intent

이 Phase에서는 실제 Work, Damage, Position을 변경하지 않는다.

## Phase 4 — Global Accounting & Nominal Demand

다음을 먼저 계산한다.

```text
Used Network Capacity
Supported Network Capacity
Overcapacity Excess
Overcapacity Support Demand
```

그 뒤 모든 정상 전력 Demand를 수집한다.

- Gate idle / switching / drive
- Wire leakage
- Wire sensing
- Live Wire
- Overcapacity support
- Relay activation / upkeep
- Mobile movement
- Extraction
- Transfer
- Construction
- Radiation emission

## Phase 5 — Power Solve & Brownout

Power Region별로 공통 `powerRatio ρ`를 결정한다.

모든 Load는 같은 Region의 `ρ`를 받는다.

## Phase 6 — Scheduling & Granted Work

- Gate pending transition 생성·취소
- Driver strength transition을 `t + 1`에 예약
- Sensor / Quartz driver sample 예약
- Movement Budget 확정
- Live Wire Energy 확정
- Radiation Emission 확정 및 Arrival 예약
- Transfer / Construction / Extraction Work Budget 확정
- Relay activation work budget 확정
- Relay upkeep health sample 확정

이미 예약된 Gate due Tick은 이후 Heat나 Brownout 변화로 수정하지 않는다.

## Phase 7 — Trajectory

Tick 시작 위치와 확정된 Movement Budget으로:

- Mobile 경로
- Enemy 경로
- Junction decision
- dead-end reverse
- swept collider

를 계산한다.

최종 위치는 아직 commit하지 않는다.

## Phase 8 — Interaction

먼저 현재 Tick에 due인 Radiation Arrival을 Cell별로 합산한다.

그 뒤 다음을 동시에 누적한다.

- Contact Electrical Energy
- Radiation absorption
- Enemy attack exposure
- Payload transfer
- Construction work
- Extraction work
- Relay activation work
- Movement heat
- Switching heat
- Overcapacity support heat
- Transmission loss heat

## Phase 9 — Thermal Integration

모든 Heat Source와 열 교환을 Phase 9 시작 상태 기준으로 동시에 계산한다.

## Phase 10 — Damage Resolution

Electrical / Thermal Exposure를 Entity별로 합산한다.

- Integrity 감소
- pending destruction 표시
- Relay destruction 표시
- Main Core destruction 표시

이 Tick에 파괴된 Entity도 현재 Tick 행동은 완료한다.

## Phase 11 — Progress Commit

- Position
- Cargo
- Construction progress
- Extraction result
- Relay activation progress
- Relay unhealthy counter
- Thermal state
- Tick State Hash

를 commit한다.

Main Core가 파괴되었다면 이 Commit 뒤 Run을 종료한다.

---

# 8. Command, Topology Change, Laboratory

## 8.1 Command Ordering

```ts
type CommandEnvelope = {
  targetTick: Tick;
  ordinal: bigint;
  command: Command;
};
```

같은 Tick의 Command는 `ordinal` 오름차순으로 처리한다.

규칙:

1. ordinal은 같은 Tick 안에서 유일해야 한다.
2. 각 Command는 앞선 accepted Command의 결과를 본다.
3. 뒤 Command가 앞 Command와 공간적으로 충돌하면 deterministic rejection을 받는다.
4. Module Placement 하나는 내부 Site 전체를 하나의 atomic command로 validate한다.
5. Command rejection은 Run Error가 아니다.

이는 Player Input의 의도적 순서를 Canonical Input Log로 인정하는 결정이다.

## 8.2 Topology Synchronization

새 route가 생성되었을 때 Sink는 다음 Driver transition까지 무기한 LOW로 남지 않는다.

Route가 제거되면 해당 Driver Slot을 즉시 제거하고 Sink를 dirty로 표시한다. Arrival이 하나도 없어도 Phase 2에서 다시 resolve한다.

Phase 0 topology rebuild 후 새로 reachable해진 `(Driver, Sink)` route마다:

```text
현재 Driver Sample
+ 현재 Driver Revision
+ 새 route wire delay
→ TopologySyncArrival
```

을 예약한다.

최소 양의 Wire Delay는 유지한다. 즉 새 Wire 연결이 같은 Tick에 즉시 Sink 값을 바꾸지는 않는다.

## 8.3 Driver Revision

각 Driver는 Sample이 바뀔 때 monotonic Revision을 증가시킨다.

Signal Arrival은 Revision을 포함한다.

Sink Slot은:

- 더 큰 Revision만 적용
- 같은 Revision duplicate는 idempotent
- 더 작은 Revision은 stale로 폐기

한다.

따라서 topology 변경 후 이전 route의 늦은 Arrival이 최신 상태를 되돌리지 않는다.

## 8.4 Path Certificate

Signal Arrival은 예약 당시 path의 정체성을 가진다.

Path Certificate는 최소 다음을 포함한다.

```text
ordered Wire EntityId
ordered Junction EntityId
connection generation
```

도착 전 하나라도:

- Entity가 제거됨
- EntityId가 다름
- connection generation이 바뀜

이면 Arrival을 폐기한다.

같은 Geometry에 재건된 새 Wire는 이전 Event를 이어받지 않는다.

## 8.5 Existing Event와 새 경로

- 기존 Event는 새 경로로 reroute하지 않는다.
- 새로 더 짧은 경로가 생겨도 기존 due Tick은 바뀌지 않는다.
- 새 route에는 current sample sync event를 별도로 보낸다.

## 8.6 Laboratory Live Edit

Laboratory와 World의 live topology semantics는 같다.

- Pause 중 Edit Command는 queue에 쌓인다.
- 다음 Single Step 또는 Resume의 Phase 0에서 적용한다.
- Host가 Canonical State를 즉시 바꾸어 보여서는 안 된다.
- UI는 ghost preview를 표시할 수 있다.
- 구조 변경과 무관한 pending event는 유지한다.
- path가 무효화된 event만 일반 규칙으로 폐기한다.

## 8.7 Laboratory Reset

Reset은 현재 Simulation을 수정하는 Command가 아니다.

```text
Reset
→ scenario initial state로 새 Simulation 생성
→ tick = 0
→ pending event 없음
```

Reset 전후는 서로 다른 Replay Session이다.

---

# 9. Physical Geometry와 Wire Body

## 9.1 하나의 Body, 네 Surface

```text
Wire Body
├─ Signal Surface
├─ Power Surface
├─ Sense Surface
└─ Track Surface
```

네 Surface는 다음을 공유한다.

- Geometry
- Physical Length
- Integrity
- Temperature
- Construction state
- World position
- Network Capacity Usage

그러나 Signal, Power flow, Sense bit, Track position을 하나의 scalar로 합치지 않는다.

## 9.2 Physical Scale Profile

최소 Profile field:

```ts
type PhysicalScaleProfile = {
  wireGeometryQuantum: Fixed;
  circuitRoutingPitch: Fixed;
  worldRoutingPitch: Fixed;
  wireBodyRadius: Fixed;
  gateFootprints: GateFootprintTable;
  gatePortAnchors: GatePortTable;
};
```

Reference Stage 0 Profile:

```text
wireGeometryQuantum = 1/64 wu
circuitRoutingPitch = 1/4 wu
worldRoutingPitch   = 1 wu
wireBodyRadius      = 1/32 wu
gate minimum box    = 1/2 wu × 1/2 wu
```

이 값은 Stage 1 Crossover의 정답을 선언하지 않는다. 모든 Parameter Sweep은 profile hash를 달리한다.

## 9.3 Circuit / World Routing

- Substrate 내부 Wire vertex는 substrate local `circuitRoutingPitch`에 정렬한다.
- Open World Wire vertex는 `worldRoutingPitch`에 정렬한다.
- 두 영역의 실제 좌표는 같은 World coordinate에 존재한다.
- Length는 동일한 Euclidean 법칙으로 계산한다.

Gate와 내부 Circuit Wire는 Fixed 또는 Mobile Substrate의 routing area에 존재한다.

World Backbone, Sensor, Track, Attack Wire는 open world에 존재할 수 있다.

## 9.4 Crossing과 Overlap

- 두 Wire가 한 점에서 교차해도 Junction이 없으면 연결되지 않는다.
- 별도 Wire Body가 양의 길이 구간을 정확히 겹치는 배치는 invalid다.
- 같은 Physical Wire를 여러 역할에 쓰려면 별도 중첩 Wire가 아니라 하나의 Wire Body를 공유해야 한다.
- Parallel Wire의 최소 centerline 간격은 해당 routing pitch다.

## 9.5 Connectivity

Junction은 incident Wire의 다음 Surface를 연결한다.

- Signal
- Power
- Track

Sense Output은 Segment-local이며 Junction에서 자동 OR되지 않는다.

## 9.6 Sense Port

각 Wire Segment는 양 endpoint에 동일한 read-only Sense Output을 제공한다.

```text
senseOutA
senseOutB
```

Main Signal Surface와는 별개다.

## 9.7 Fixed / Mobile Substrate

Substrate는 계산하지 않는다. 실제 Circuit을 놓을 수 있는 routing area와 physical body를 제공한다.

- Fixed Substrate의 local geometry는 World에 고정된다.
- Mobile Substrate의 internal Circuit은 substrate-local geometry로 저장된다.
- Gate와 Circuit Internal Wire의 Footprint는 Substrate Footprint에 포함된다.
- Substrate가 파괴되어 내부 Primitive가 물리적 지지를 잃으면 해당 Primitive도 다음 Phase 0에 제거 대상으로 처리한다.

Mobile Substrate의 World collider는 internal Circuit bounding box와 Payload area를 포함한다.

---

# 10. Signal Semantics

## 10.1 Logic Domain

```text
LOW
HIGH
X
```

`X`는 다음을 의미한다.

- 반대 Drive 충돌
- Logic Threshold 미달
- X Driver 영향

## 10.2 Driver Sample

```ts
type DriverSample = {
  level: "LOW" | "HIGH" | "X";
  strength: DriveStrength;
  revision: Revision;
  emittedAt: Tick;
  driverId: EntityId;
};
```

## 10.3 Passive Default

Sink에 유효 Driver가 없으면 LOW로 resolve한다.

이는 passive pull-down World Rule이다.

## 10.4 Drive Resolution

```text
H = HIGH strength 합
L = LOW strength 합
U = X strength 합
Θ = logicThreshold
```

```text
U >= Θ                         → X
H >= Θ and L >= Θ              → X
H >= Θ                         → HIGH
L >= Θ                         → LOW
otherwise                      → LOW
```

HIGH와 LOW가 동시에 존재하면 `min(H, L)`에 비례한 contention heat가 발생한다.

## 10.5 Independent Driver

모든 Gate Output은 새로운 독립 Driver다.

따라서:

```text
loaded net
→ NOT
→ NOT
→ new net
```

은 논리값을 보존하면서 Fan-out load를 끊는다.

별도 Buffer Primitive는 없다.

---

# 11. Gate Semantics

## 11.1 Arity

- AND: 2 Input / 1 Output
- OR: 2 Input / 1 Output
- NOT: 1 Input / 1 Output

모든 Gate는 Power Tap을 가진다.

## 11.2 Truth Table with X

### AND

| A | B | OUT |
|---|---|---|
| LOW | * | LOW |
| * | LOW | LOW |
| HIGH | HIGH | HIGH |
| 그 외 | | X |

### OR

| A | B | OUT |
|---|---|---|
| HIGH | * | HIGH |
| * | HIGH | HIGH |
| LOW | LOW | LOW |
| 그 외 | | X |

### NOT

| IN | OUT |
|---|---|
| LOW | HIGH |
| HIGH | LOW |
| X | X |

## 11.3 Initial State

새 Gate 활성화 시:

```text
internal output = LOW
driver strength = 0
pending transition = none
```

Power를 받은 뒤 현재 Input을 평가하고 일반 Delay로 출력한다.

Engine은 Feedback Circuit을 자동 안정화하지 않는다.

## 11.4 Effective Delay

```text
gateDelay = max(
  1,
  ceil(
    (gateBaseDelay + fanoutPenalty(load))
    × thermalDelayFactor(T)
    × brownoutDelayFactor(ρ)
  )
)
```

Output load:

```text
inputLoad × reachableSinkCount
+
wireLoadPerLength × totalConnectedWireLength
```

Gate를 통과하면 load는 reset된다.

`fanoutPenalty`는 단조 증가해야 한다.

충분히 큰 load에서는 `NOT → NOT` 2 Gate 비용보다 직접 Fan-out penalty가 커지는 구간이 MUST 존재한다.

## 11.5 Inertial Delay

1. Input으로 desired output 계산
2. desired != current면 `t + delay` transition 예약
3. due 전 desired가 바뀌면 기존 예약 무효화
4. desired가 current로 돌아오면 예약 취소
5. 취소된 switching energy는 환불되지 않고 Heat로 남음

Gate Delay보다 짧은 Pulse는 제거될 수 있다.

## 11.6 Switching Power와 Drive

```text
switchEnergy = gateSwitchBaseEnergy × (1 + load)
```

```text
outputStrength =
  nominalGateDrive
  × powerRatio
  × thermalDriveFactor(T)
```

`powerRatio < logicOperateThreshold`이면 새로운 Logic transition을 예약하지 않는다.

장기간 무전력 시 내부 state retention 뒤 LOW로 초기화한다. Retention Tick은 Balance Parameter다.

## 11.7 Driver Strength Response

Logic Level이 같더라도 Brownout이나 Heat로 effective Drive Strength가 달라지면 새로운 Driver Sample Revision이 필요하다.

Phase 6에서 현재 active strength와 새 effective strength가 다르면:

```text
DriverStrengthTransition due = t + 1
```

을 예약한다.

- Level 또는 Strength 중 하나라도 바뀌면 Driver Revision이 증가한다.
- Source Wire의 현재 Tick 행동은 Phase 2에서 이미 active한 Sample을 사용한다.
- Sink는 일반 Wire Transport Delay 뒤 새 Strength를 관찰한다.
- Strength 변화가 같은 Tick에 원격 Sink로 즉시 전파되어서는 안 된다.

---

# 12. Wire Delay, Fan-out, Feedback

## 12.1 Canonical Signal Path

Driver에서 Sink까지 canonical shortest route를 사용한다.

우선순위:

1. 총 Euclidean Wire Length
2. Segment 수
3. ordered EntityId lexicographic order

같은 Driver Revision은 하나의 Sink에 한 번만 적용한다.

Wire Loop가 echo를 무한 생성하지 않는다.

## 12.2 Superlinear Wire Delay

Unbuffered path length `L`:

```text
wireDelay(L) = max(
  1,
  ceil(
    (wireLinearK × L + wireQuadraticK × L²)
    × pathThermalFactor
  )
)
```

`wireQuadraticK > 0` MUST.

목적:

- 장거리 중앙집중 비용
- Local Logic 가치
- Buffer-like re-drive 가치
- Layout 최적화
- Feedback 비대칭

## 12.3 Transport Delay

Wire는 실제 Driver Transition을 삭제하지 않는다.

Source에서 발생한 1 Tick Pulse는 Wire Delay 뒤에도 1 Tick Pulse로 도착한다.

## 12.4 Feedback

Output을 upstream Input으로 연결할 수 있다.

Feedback은 특별 Recipe가 아니다.

모든 path에 양의 Delay가 있으므로 Tick에 따라 진화한다.

## 12.5 Startup

t=0 또는 Construction activation 시:

- Gate output LOW
- Driver strength 0
- Sink passive LOW
- Sense sample LOW
- pending event 없음

완전히 대칭인 Feedback 회로는 진동하거나 X가 될 수 있다.

Reset / Initialization / Startup Sequence는 플레이어가 설계한다.

## 12.6 Glitch / Race / Hazard

Path Delay 차이에서 나온 Pulse는 실제 Signal이다.

다음에 연결되면 실제 World 행동을 만든다.

- Live Wire
- Mobile control
- BUILD
- Extraction
- Radiation
- Feedback state

Module 내부 event를 memoize하여 제거하는 최적화는 금지한다.

---

# 13. Global Network Capacity

## 13.1 Capacity Unit

```text
1 NCU = 1 world unit의 active Physical Wire centerline length
```

Canonical 내부에서는 NCU를 Fixed로 표현한다.

따라서 0.25 world unit Wire는 0.25 NCU를 사용한다.

UI 표시를 위해 Wire별로 정수 반올림해서는 안 된다.

## 13.2 Used Capacity

Tick `t`의 Used Capacity:

```text
U(t) = Σ activeWireLength_e
```

포함:

- World Backbone Wire
- Circuit Internal Wire
- Sensor Wire
- Track Wire
- Contact Attack Wire
- Radiation Wire
- Fixed Substrate Circuit Wire
- Mobile Substrate Circuit Wire

제외:

- Construction 완료 전 Wire Site
- 이미 Phase 0에서 제거된 Wire
- Wire가 아닌 Gate / Junction / Substrate Body

Phase 10에서 pending destruction이 된 Wire는 현재 Tick까지 Active이며, 다음 Phase 0 제거 시 Usage에서 빠진다.

Gate 자체는 Network Capacity를 직접 사용하지 않는다. Gate 사이를 연결하는 실제 Wire가 사용한다.

## 13.3 Multi-role Accounting

하나의 Wire Body가 Signal / Power / Sense / Track을 동시에 사용해도 Length를 한 번만 계산한다.

```text
one body
four roles
→ one length charge
```

별도 Wire Body를 겹쳐 여러 채널을 숨기는 것은 §9.4에 의해 허용되지 않는다.

## 13.4 Length Preservation

같은 Geometry의 Wire를 Junction에서 여러 Segment로 나누어도 총 Length가 같다면 Used Capacity는 같아야 한다.

Polyline vertex를 불필요하게 추가해도 Euclidean centerline 합이 같으면 Capacity가 달라져서는 안 된다.

## 13.5 Supported Capacity

```text
S(t) = MainCoreCapacity
     + Σ capacity(relay_i where mode == ONLINE)
```

Main Core가 살아 있는 동안 Main Core Capacity를 제공한다.

Relay는 현재 Tick Phase 0에서 ONLINE인 경우에만 기여한다.

## 13.6 Overcapacity

```text
E = max(0, U - S)
```

`E = 0`이면 Overcapacity Support Demand는 0이다.

`E > 0`이어도:

- Wire Build를 거부하지 않는다.
- Existing Wire를 삭제하지 않는다.
- Signal을 직접 느리게 만들지 않는다.
- Capacity 전용 Damage를 발생시키지 않는다.

## 13.7 Soft Support Curve

Total Overcapacity Support Demand:

```text
D_support = supportPowerPerNCU
          × (
              overcapLinearK × E
              + overcapQuadraticK × E² / max(S, capacityDenominatorFloor)
            )
```

규칙:

1. `D_support(0) = 0`
2. E에 대해 단조 증가 MUST
3. `supportPowerPerNCU > 0`
4. `overcapLinearK >= 0`
5. `overcapQuadraticK > 0`
6. 최종 Energy Demand는 `ceil_div_nonnegative`로 올림한다.
7. Build Permission과 분리한다.
8. coefficient는 Balance Profile이다.

이 함수 형태가 Capacity의 정본이다.

## 13.8 Support Demand Distribution

`U > 0`일 때 Wire `e`의 Support Demand:

```text
D_support_e = D_support × length_e / U
```

정수 remainder는 Wire EntityId 오름차순으로 1 Energy unit씩 배분한다.

각 Wire의 Support Demand는 해당 Wire가 속한 Power Region의 intrinsic, non-switchable Load다.

따라서 abandoned Wire가 전체 Overcapacity Ratio를 올리면 정상 Network의 Wire도 더 큰 Support Demand를 받는다.

## 13.9 Support Heat

실제로 공급된 Support Energy 중:

```text
grantedSupportEnergy × supportHeatFraction
```

은 해당 Wire의 Heat가 된다.

```text
0 < supportHeatFraction <= 1
```

이어야 한다.

나머지는 모델링하지 않는 유지 손실로 소실된다.

Overcapacity는 기존 Power / Heat / Brownout을 통해 Timing과 생존성을 악화시킨다.

## 13.10 Relay Loss

Relay 파괴 또는 Offline으로 `S`가 감소해도 `U`는 바뀌지 않는다.

다음 Tick Phase 4에서 더 큰 `E`와 `D_support`가 계산된다.

이것이 Relay Loss에 따른 Network Crisis다.

## 13.11 Global Pool

v1.0은 Regional Capacity를 계산하지 않는다.

Wire의 위치와 Relay까지의 거리에 따라 Capacity를 귀속하지 않는다.

지리적 비용은 다음 기존 시스템이 담당한다.

- actual Wire Length
- Power transmission distance
- Construction Work
- Repair Distance
- Damage Exposure

---

# 14. Main Core와 Relay

## 14.1 Main Core

Main Core는:

- Run 종료 조건
- Global Network Capacity source
- Network anchor root

다.

Main Core는 Power Source가 아니다.

Main Core가 파괴되면 Phase 11 Commit 뒤 Run이 종료된다.

## 14.2 Relay Site

Relay Site는 World Generation이 만든 immutable location이다.

- 새 Relay Site를 만들 수 없다.
- Relay Structure가 파괴되어도 Site는 남는다.
- 해당 Site에서만 Relay를 reconstruction할 수 있다.

Reference Stage 2 World는 각 Site에 intact but OFFLINE Relay Structure를 둔다.

## 14.3 Relay는 계산하지 않는다

Relay는 다음을 수행하지 않는다.

```text
Signal Processing
Routing
Pathfinding
Targeting
Power Priority
Automatic Buffering
```

Relay의 유일한 systemic output은 ONLINE일 때의 Capacity contribution이다.

## 14.4 Anchor Connectivity

Relay가 activate 또는 online 유지되려면 Relay attachment에서 Main Core Network Anchor까지 살아 있는 Wire/Junction Body path가 있어야 한다.

이 판정은 Physical Body connectivity다.

Relay가 Signal을 처리하거나 중계한다는 의미가 아니다.

## 14.5 Relay State

```text
OFFLINE
ONLINE
DESTROYED
```

`DESTROYED`는 Relay Structure가 없는 상태이며 Site는 존재한다.

`ACTIVATING`은 별도 Capacity state가 아니다. OFFLINE Relay가 현재 Activation Work를 받고 있을 때 Analyzer가 표시할 수 있는 derived label이다.

## 14.6 Activation

OFFLINE Relay가 다음을 만족하면 Activation Work를 제출한다.

- Structure intact
- Anchor connected
- Activation Power path 존재

```text
activationProgress += grantedActivationWork
```

중간에 Power나 연결이 끊겨도 progress는 유지한다.

처음 한 번도 ONLINE이 아니었던 Structure의 target은 `relayActivationWork`, 이전에 ONLINE이었다가 OFFLINE이 된 Structure의 target은 `relayRestartWork`다.

```text
activationProgress >= currentActivationTarget
→ ONLINE transition을 다음 Tick Phase 0에 예약
```

ONLINE 전환 시 `activationProgress = 0`, `hasEverBeenOnline = true`로 commit한다.

ONLINE 전환 전 Tick에는 Capacity를 제공하지 않는다.

## 14.7 Online Upkeep

ONLINE Relay는 매 Tick intrinsic upkeep demand를 제출한다.

Relay health sample:

```text
healthy =
  anchorConnected
  and grantedUpkeep >= relayHoldThreshold
```

- healthy면 `unhealthyTicks = 0`
- healthy가 아니면 `unhealthyTicks += 1`

```text
unhealthyTicks >= relayOfflineGraceTicks
→ OFFLINE transition을 다음 Tick Phase 0에 예약
```

`relayOfflineGraceTicks >= 1` MUST다.

OFFLINE이 되면 Capacity contribution은 그 Phase 0부터 사라진다.

Activation Work와 Hold Threshold를 분리하여 Relay가 한 Tick 단위로 무한 토글되는 것을 막는다.

## 14.8 Restart

Power나 connection loss로 OFFLINE이 된 Relay는 `relayRestartWork`를 다시 충족해야 ONLINE이 된다.

OFFLINE 전환 시 restart progress는 0에서 시작한다.

Initial activation과 restart의 Work 값은 Balance Profile이 다르게 둘 수 있다.

## 14.9 Destruction

Relay Integrity가 Phase 10에서 0 이하가 되면:

- pending destruction
- 다음 Tick Phase 0에 ONLINE contribution 제거
- Relay Structure 제거
- Relay Site에 Reconstruction Site 생성
- activation / unhealthy progress 초기화

Adjacent Wire는 자동 삭제하지 않는다.

## 14.10 Reconstruction

Relay Reconstruction은 고수준 `REPAIR_RELAY`가 아니다.

해당 Relay Site의 Reconstruction Site에:

- required cargo
- construction power
- BUILD work

를 공급한다.

완료되면 다음 Tick Phase 0에 OFFLINE Relay Structure가 활성화된다.

새 Structure는 다시 activation해야 한다.

---

# 15. Power Network와 Brownout

## 15.1 Electricity는 Flow다

Power는 Inventory가 아니다.

각 Tick에 생산되고 소비된다.

Power Surface가 연결된 Wire / Junction / Device는 Power Region을 이룬다.

## 15.2 Nominal Demand

대표 Load:

- Gate idle
- Gate switching
- Gate output drive
- Wire leakage
- Wire sensing
- Live Wire
- Overcapacity support
- Relay activation / upkeep
- Mobile movement
- Extraction
- Transfer
- Construction
- Radiation emission

모든 Demand는 먼저 수집하고 iteration order와 무관하게 solve한다.

## 15.3 Canonical Power Path

Load에서 연결 가능한 Power Source까지:

1. 짧은 Euclidean Length
2. 적은 Segment
3. EntityId lexicographic

순서로 source path를 선택한다.

Power Region의 Generation은 합산한다.

연결 가능한 Power Source가 없는 Region은 `G = 0`, `ρ = 0`이다.

## 15.4 Transmission Loss

Load `i`에 전달되는 Power:

```text
P_i = ρ × D_i
```

Source cost:

```text
sourceCost_i =
  P_i
  + powerLossK × distance_i × P_i²
```

두 번째 항은 path Wire에 Length 비례 Heat로 분배한다.

## 15.5 Region Brownout Ratio

Region Generation `G`에 대해 다음을 만족하는 최대 `ρ ∈ [0,1]`을 결정한다.

```text
Σ sourceCost_i(ρ) <= G
```

모든 Load는 같은 Region `ρ`를 받는다.

먼저 순회된 Load가 Power를 독점해서는 안 된다.

## 15.6 Brownout Effects

`ρ`는 다음에 영향을 준다.

- Gate Delay
- Gate Drive
- Sensor Drive
- Live Wire Energy
- Radiation Energy
- Movement Speed
- Extraction / Construction Work
- Relay activation / upkeep

`ρ < logicOperateThreshold`이면 Logic Driver는 유효 Drive를 제공하지 못한다.

## 15.7 Load Shedding

Engine은 Priority Scheduler를 제공하지 않는다.

플레이어는 Circuit으로:

- Defense OFF
- Mobile STOP
- Extraction STOP
- Construction STOP
- Power Region 분리

를 수행한다.

Overcapacity Support Demand와 Relay Upkeep는 intrinsic load이며 Signal Port로 직접 끌 수 없다.

---

# 16. Leakage와 Wire Heat

Baseline Wire Leakage:

```text
wireLeakage =
  leakagePerLength
  × wireLength
  × leakageThermalFactor(T)
```

Signal switching:

```text
signalPower ≈ switchingActivity × load
```

HIGH Wire의 사용되지 않은 Live Energy는 대부분 Wire Heat가 된다.

Overcapacity Support의 granted energy 일부도 Wire Heat가 된다.

따라서 Wire를 더 많이 유지하는 비용은 다음 네 축에서 나타난다.

- Capacity Usage
- Leakage
- Power Loss
- Heat

---

# 17. Wire Sensing

## 17.1 Geometry

각 Wire Segment의 sensing region은 Wire polyline을 `senseRadius`만큼 확장한 capsule이다.

```text
presence = HIGH
iff
Hostile collider intersects capsule
```

적 수와 무관하게 1bit다.

## 17.2 제공하지 않는 정보

- 정확한 좌표
- 거리
- 적 수
- 속도
- 방향
- HP
- Target
- Enemy Type

## 17.3 Sampling Delay

Phase 1 위치를 sample하고 `senseDelay` 뒤 Sense Driver에 전달한다.

짧은 Occupancy Pulse도 transport 방식으로 보존한다.

## 17.4 Power Failure

Sense Surface는 Power를 소비한다.

Driver Strength가 Logic Threshold에 못 미치면 Sink에는 passive LOW로 보인다.

별도 `SENSOR_HEALTH` bit는 없다.

---

# 18. Mobile Substrate와 Mobility

## 18.1 Track Position

```ts
type TrackPosition =
  | { edgeId: EntityId; offset: Fixed; heading: 1 | -1 }
  | { junctionId: EntityId; incomingEdgeId: EntityId };
```

Mobile은 Wire Track Surface 위에서만 이동한다.

## 18.2 Intrinsic Ports

```text
STOP
LEFT
RIGHT
LOAD
UNLOAD
BUILD
```

다음은 없다.

```text
MOVE_TO
PATHFIND
REPAIR
DELIVER_TO
```

## 18.3 Footprint와 Mass

```text
mobileFootprint =
  boundingBox(internal Gate / Wire geometry)
  + payloadArea
  + profileClearance
```

Footprint는:

- physical collider
- Damage exposure
- Construction Work
- Mass

에 사용한다.

Mass:

- Substrate body
- Gate
- Internal Wire
- Payload A/O/N
- 기타 cargo

## 18.4 Movement

```text
movementBudget =
  baseMovePerTick
  × powerRatio
  ÷ massFactor(totalMass)
```

## 18.5 Junction Decision

Control은 Phase 3에서 한 번 sample한다.

`STOP = HIGH` 또는 필요한 control이 X면 정지한다.

STOP LOW:

| LEFT | RIGHT | 결과 |
|---|---|---|
| LOW | LOW | 가장 straight에 가까운 edge |
| HIGH | LOW | 가장 left edge |
| LOW | HIGH | 가장 right edge |
| HIGH | HIGH | reverse |

Tie-break:

1. turn angle
2. EntityId

Degree 1 dead-end에서 `00`은 reverse한다.

## 18.6 Power Boundary

다음 Track Segment가 무전력이면 경계에서 정지한다.

v1.0에는 inertia 또는 battery coast가 없다.

## 18.7 Multiple Mobile

v1.0에서 Mobile끼리는 Track capacity를 점유하지 않고 통과할 수 있다.

Traffic와 collision은 별도 revision이다.

---

# 19. Payload, Transfer, Construction

## 19.1 Payload

Mobile Payload는 실제 cargo unit을 가진다.

Payload는 Footprint와 Mass를 증가시킨다.

## 19.2 Transfer

`LOAD` 또는 `UNLOAD`가 HIGH이고 compatible endpoint와 겹치면 Transfer Work를 수행한다.

여러 endpoint가 겹치면 EntityId가 작은 하나를 선택한다.

여러 Mobile이 같은 inventory에 접근하면 모든 intent를 먼저 수집한 뒤 Mobile EntityId 순서로 available unit을 배정한다.

## 19.3 Construction Site

Construction은 즉시 Entity를 만들지 않는다.

```ts
type ConstructionSite = {
  targetKind: string;
  exactGeometry: Geometry;
  requiredCargo: Cargo[];
  requiredWork: Energy;
  suppliedCargo: Cargo[];
  completedWork: Energy;
};
```

## 19.4 Required Work

```text
Gate Work       = gateWorkByType
Junction Work   = junctionBaseWork
Wire Work       = wireEndpointWork + wireWorkPerNCU × wireLength
Substrate Work  = substrateWorkPerArea × area
Relay Work      = relayReconstructionWork
```

긴 Wire는 반드시 더 많은 Work를 요구한다.

## 19.5 Build Work

`BUILD = HIGH`인 Mobile이 Site와 겹치면 granted Construction Energy를 Work로 누적한다.

여러 Builder의 Work는 같은 Tick에 합산한다.

Cargo requirement와 Work requirement를 모두 만족하면 다음 Tick Phase 0에 target을 활성화한다.

## 19.6 Capacity와 Partial Construction

완료 전 Wire Site는:

- Signal / Power / Sense / Track 기능 없음
- Network Capacity Usage 없음

완료되어 Active Wire가 된 Phase 0부터 전체 Length가 Used Capacity에 들어간다.

Capacity 부족은 Construction 완료를 막지 않는다.

## 19.7 A/O/N Gate Cargo

A/O/N Gate는 matching A/O/N cargo를 요구한다.

Wire / Junction / Substrate는 A/O/N cargo를 요구하지 않지만 Work와 Power를 요구한다.

Relay Reconstruction cargo는 Balance Profile이 정의하며 Reference Profile은 cargo 없이 Work와 Power만 요구한다.

## 19.8 Reconstruction

Gate 또는 Wire 파괴 시 같은 위치와 Geometry의 Reconstruction Site를 생성한다.

Engine은 다음만 보존한다.

- 파괴된 Primitive kind
- exact geometry
- required cargo
- required work

Fault detection, cargo 확보, route, BUILD는 Player Circuit 책임이다.

## 19.9 Module Placement

Module Placement는 내부 Primitive별 Site를 생성한다.

완성된 Primitive부터 다음 Tick에 동작한다.

Module 전체가 완성될 때까지 black box로 비활성화하지 않는다.

## 19.10 Run Boundary

Main Core가 파괴되어 Run이 끝나면 Canonical World State는 종료된다.

다음 Run으로 자동 이전되는 Canonical State는 없다.

Module Library는 Run 바깥의 persistent artifact store이며 다음만 보존한다.

- Blueprint geometry
- Primitive composition
- I/O binding
- provenance
- compatible profile identifiers

Power, Capacity, A/O/N inventory, Relay state, installed Circuit은 계승하지 않는다.

---

# 20. Attack Ontology와 Contact Attack

## 20.1 Damage Type

```text
Electrical
Thermal
```

Projectile과 Radiation은 전달 방식이다.

## 20.2 Live Wire

Wire resolved Signal이 HIGH이고 유효 Drive가 있으면 Live Energy Demand를 제출한다.

```text
liveDemand =
  liveEnergyPerStrengthLength
  × highDriveStrength
  × segmentLength
```

일반 Logic Drive는 작아서 공격력이 작다.

같은 HIGH Driver를 병렬 연결하면 Strength가 합쳐질 수 있다.

반대 Driver는 X와 contention heat를 만든다.

## 20.3 Contact

Enemy 또는 외부 damageable collider의 swept volume이 actual Wire Body와 교차하면 Contact가 발생한다.

Sensing Radius가 아니라 Wire Body Radius를 사용한다.

## 20.4 Energy Allocation

```text
weight_i =
  contactDuration_i
  × conductivity_i
  × contactMeasure_i
```

```text
absorbed_i =
  liveEnergy
  × weight_i
  ÷ (worldLeakWeight + Σ weight)
```

Remainder는 EntityId 순서로 배분한다.

총 흡수 Energy는 granted Live Energy를 넘지 않는다.

나머지는 Wire Heat다.

## 20.5 Friendly Fire

Faction immunity는 없다.

정상 attach된 자기 Gate / Junction / Substrate는 contact target에서 제외한다.

외부에서 겹친 Player Entity는 피해를 받을 수 있다.

---

# 21. Radiation

## 21.1 원리

Radiation은 연속 전자기장 Simulation이 아니다.

> **Wire Drive transition의 Switching Energy가 Wire Geometry와 정수 Spatial Kernel에 따라 주변 공간으로 전달되는 결정론적 이산 Energy 전달이다.**

DC HIGH / LOW는 지속 Radiation을 만들지 않는다.

## 21.2 Switching Source

```text
HIGH → +strength
LOW  → -strength
X / no drive → 0
```

```text
Δdrive(t) = signedDrive(t) - signedDrive(t-1)
```

`Δdrive = 0`이면 Radiation emission intent가 없다.

Source는 전체 local power draw가 아니라 Switching Energy다.

## 21.3 Emission

Radiation Source 단위는 Wire polyline을 구성하는 canonical straight segment다. 긴 polyline은 각 straight segment가 별도 Geometry와 Kernel을 가진다.

```text
radiationDemand =
  f(abs(Δdrive), segmentLength, connectedLoad)
```

Power Solve 뒤:

```text
0 <= emittedEnergy <= grantedSwitchingEnergy
```

방사되지 않은 Energy와 inefficiency는 Wire Heat가 된다.

## 21.4 Canonical Cell

```text
cellX = floor_div(worldX, radiationCellSize)
cellY = floor_div(worldY, radiationCellSize)
```

Cell은 Gameplay Primitive가 아니다.

## 21.5 Integer Kernel

```text
rawWeight(cell) =
  distanceWeight(distanceBand)
  × orientationWeight(orientationBand)
```

규칙:

- distance가 멀수록 weight 단조 감소
- orientation에 따라 분포 변화
- 정수 Lookup Table
- finite radius
- runtime float / atan2 / GPU reduction 금지

Orientation bin은 fixed-point vector의 정수 비교로 결정한다.

## 21.6 Source Budget Allocation

```text
W = escapeWeight + Σ rawWeight(cell)
```

```text
cellEnergy = emittedEnergy × rawWeight / W
```

Remainder 우선순위:

1. raw weight 큰 Cell
2. distance band 짧은 Cell
3. `(cellY, cellX)` lexicographic

```text
Σ cellEnergy <= emittedEnergy
```

## 21.7 Propagation Delay

```text
arrivalTick =
  emissionTick
  + radiationDelay(distanceBand)
```

`radiationDelay >= 1 Tick`, 단조 비감소다.

한 번 예약된 Arrival은 Source Wire가 이후 파괴되어도 유지한다.

## 21.8 Arrival Accumulation

같은 Cell과 같은 Tick의 Energy는 합산한다.

Source sign이나 continuous phase angle로 cancellation하지 않는다.

A/O/N에서 Timing Control은 여러 Source의 Energy를 같은 Cell·Tick에 도착시키는 것이다.

## 21.9 Absorption

```text
targetWeight =
  targetAbsorption
  × targetCrossSection
  × cellCoverage
```

```text
absorbed_i =
  cellArrivalEnergy
  × targetWeight_i
  ÷ (worldEscapeWeight + Σ targetWeight)
```

총 흡수는 Cell Arrival Energy 이하이다.

흡수 Energy는 Electrical / Thermal로 분할된다.

## 21.10 Debug

Laboratory는 다음을 표시할 수 있어야 한다.

- Switching Event
- Source Emission Energy
- Kernel footprint
- Arrival Tick
- Cell Energy
- Same-tick accumulation
- Target absorption

Sensing과 Spatial Index를 공유할 수 있지만 Semantics는 별개다.

---

# 22. Thermal Model

## 22.1 Thermal State

```text
Temperature = HeatEnergy / ThermalCapacity
```

## 22.2 Heat Source

- Gate idle / switching
- Wire leakage
- Signal contention
- Power transmission loss
- Overcapacity support
- unused Live Energy
- Contact remainder
- Radiation inefficiency
- Movement
- Extraction
- Construction
- Enemy thermal attack

## 22.3 Simultaneous Exchange

각 thermal edge `(a,b)`의 ideal transfer:

```text
q_ideal = conductance × abs(Ta - Tb)
```

높은 Temperature에서 낮은 Temperature 방향으로 전달한다.

모든 `q_ideal`은 Phase 9 시작 상태에서 계산한다.

한 Thermal Object의 total outgoing이 현재 HeatEnergy를 넘으면 모든 outgoing edge를 같은 비율로 축소한다.

```text
q_granted_e =
  q_ideal_e
  × availableHeat
  ÷ totalIdealOutgoing
```

정수 remainder는 destination key와 edge id의 lexicographic order로 배분한다.

그 뒤 모든 granted transfer를 동시에 적용한다.

따라서:

- HeatEnergy는 음수가 되지 않는다.
- pairwise transfer는 Energy를 생성하지 않는다.
- iteration order가 결과를 바꾸지 않는다.

Ambient cooling은 infinite ambient sink/source와의 thermal edge로 같은 staging 규칙을 사용한다.

## 22.4 Heat Effect

Tick 시작 Temperature는:

- Gate Delay
- Gate Drive
- Wire Delay
- Leakage
- Thermal Damage

에 영향을 준다.

현재 Tick에 새로 생긴 Heat는 다음 Tick Timing에 영향을 준다.

---

# 23. Damage와 Destruction

## 23.1 Electrical Damage

```text
electricalDamage =
  electricalEnergy / electricalTolerance
```

## 23.2 Thermal Damage

```text
thermalDamage =
  thermalDamageRate
  × max(0, T - safeTemperature)
```

## 23.3 Integrity

```text
integrityNext =
  integrity
  - electricalDamage
  - thermalDamage
```

부분 Integrity는 v1.0에서 Timing이나 Drive를 직접 변경하지 않는다.

성능 저하는 Heat와 Brownout이 담당한다.

## 23.4 Simultaneous Destruction

Phase 10에서 Integrity <= 0이면 pending destruction이다.

실제 제거는 다음 Tick Phase 0이다.

같은 Tick에 서로를 파괴한 Entity는 둘 다 현재 Tick 행동을 완료한다.

## 23.5 Wire Destruction

Wire 제거 시 동시에 끊긴다.

- Signal
- Power
- Sense
- Track
- Network Capacity Usage

Path Certificate가 무효인 Signal Arrival은 폐기한다.

이미 방출된 Radiation Arrival은 유지한다.

## 23.6 Main Core

Main Core Integrity <= 0이면 해당 Tick Commit 뒤 Run End다.

## 23.7 Relay

Relay 파괴는 §14.9를 따른다.

---

# 24. Quartz

Quartz는 안정적인 Timing Reference다.

## 24.1 Output

```text
phase = worldTick mod quartzPeriod
LOW  if phase < quartzPeriod / 2
HIGH otherwise
```

Period는 짝수다.

## 24.2 Stability

Quartz period와 internal phase는:

- Fan-out
- Wire Length
- Brownout
- Heat

에 흔들리지 않는다.

Power가 부족하면 Output Drive Strength만 0이 된다.

Power 복구 시 현재 World phase를 출력한다.

## 24.3 Quartz 없는 Clock

Feedback Oscillator는 Quartz 없이 만들 수 있다.

그 period는 Gate / Wire Delay, Heat, Brownout에 따라 변한다.

---

# 25. Determinism Contract

## 25.1 동일 입력, 동일 결과

다음이 같으면 Tick별 Canonical State Hash가 같아야 한다.

- Initial World
- Command Log
- deterministic World / Enemy input
- Semantics Version
- Numeric Profile
- Physical Scale Profile
- Balance Profile

## 25.2 금지된 비결정성

- HashMap iteration order
- thread scheduling
- GPU float reduction
- OS timer
- rendering FPS
- CPU core count
- locale
- pointer address
- wall-clock timestamp

## 25.3 Tie-break

기본 우선순위:

1. 짧은 distance
2. 적은 segment
3. 작은 EntityId
4. lexicographic path / cell key

각 subsystem이 더 구체적인 순서를 정의하면 그 규칙이 우선한다.

## 25.4 Randomness

Simulation Semantics에 implicit random draw는 없다.

World Generation과 Enemy Content randomness는:

- versioned PRNG + seed
- explicit input event

중 하나여야 한다.

## 25.5 Replay Header

```ts
type ReplayHeader = {
  semanticsVersion: string;
  numericProfileHash: string;
  physicalScaleProfileHash: string;
  balanceProfileHash: string;
  worldGeneratorVersion: string;
  seed: string;
  initialStateHash: string;
};
```

---

# 26. Analyzer와 관찰 가능성

Analyzer는 Circuit의 의미를 결정하지 않는다.

제공 가능한 값:

## Physical

- A/O/N Count
- Footprint
- Wire Length
- Network Capacity Usage
- Fan-out
- Delay
- Power
- Heat
- Construction Work

## Behavioral

- Stateful
- Periodic
- Stable / Unstable
- Reachable State Count
- Edge-sensitive

## Network

- Supported Capacity
- Used Capacity
- Excess
- Overcapacity Support Demand
- Relay state
- Power Margin

## Radiation

- Emission
- Kernel
- Arrival
- Absorption

다음 이름은 Analyzer가 자동 부여하지 않는다.

```text
CPU
Memory
Repair Brain
Router
Fire Controller
```

---

# 27. Reference Profiles

## 27.1 Numeric Profile v1

```text
fixedScale = 65,536
coordinate floor = mathematical floor
segment length = ceil integer Euclidean sqrt
fixed coefficient rounding = nearest, ties to even
overflow = deterministic error
```

## 27.2 Stage 0 Physical Scale Profile alpha

| Parameter | Value |
|---|---:|
| wireGeometryQuantum | 1/64 wu |
| circuitRoutingPitch | 1/4 wu |
| worldRoutingPitch | 1 wu |
| wireBodyRadius | 1/32 wu |
| minimum Gate box | 1/2 × 1/2 wu |

이 값은 Stage 1 Crossover를 확정하지 않는다.

## 27.3 Stage 0 Balance Profile alpha

| Parameter | Value |
|---|---:|
| simulationHz | 20 |
| gateBaseDelay | 1 tick |
| senseDelay | 1 tick |
| logicThreshold | 100 drive |
| nominalGateDrive | 400 drive |
| inputLoad | 1 |
| wireLoadPerWU | 1 |
| fanoutFreeLoad | 4 |
| fanoutStep | 4 |
| wireLinearK | 0.10 tick / wu |
| wireQuadraticK | 0.025 tick / wu² |
| logicOperateThreshold | 0.20 |
| brownoutDelayFloor | 0.20 |
| senseRadius | 1.25 wu |
| quartzPeriod | 8 ticks |
| radiationCellSize | 1 wu |

## 27.4 Capacity Probe Profile alpha

다음 값은 Stage 1 headless probe의 시작점일 뿐 제품 밸런스 확정값이 아니다.

| Parameter | Initial Probe Value |
|---|---:|
| Main Core Capacity | 1000 NCU |
| Relay Capacity | 500 NCU |
| overcapLinearK | 1.0 |
| overcapQuadraticK | 2.0 |
| capacityDenominatorFloor | 1 NCU |
| relayOfflineGraceTicks | 1 |
| supportHeatFraction | 0보다 크고 1 이하 |

`supportPowerPerNCU`, Relay Activation / Upkeep, Gate Footprint와 Routing Pitch는 Parameter Sweep 대상이다.

각 Sweep run은 고유 profile hash를 가진다.

## 27.5 Reference Radiation Table

```text
distanceWeight:
0 → 16
1 → 8
2 → 4
3 → 2
4 → 1
else → 0

radiationDelay:
0 → 1 tick
1 → 1 tick
2 → 2 ticks
3 → 3 ticks
4 → 4 ticks

orientationWeight:
broadside-near → 4
diagonal       → 2
endfire-near   → 1
```

Orientation bin boundary는 profile table에 정수 vector condition으로 저장한다.

---

# 28. Conformance Tests

## C-01 — Gate + Wire Delay

조건:

- NOT delay 1
- Wire delay 3
- Input LOW→HIGH at t=0

기대:

- internal transition t=1
- Sink arrival t=4

## C-02 — Inertial Filtering

Gate delay 3, HIGH pulse 2 Tick.

기대: Output pulse 없음, cancelled switching energy는 Heat.

## C-03 — Wire Transport

1 Tick Source pulse, Wire delay 5.

기대: Sink에서 5 Tick 뒤 1 Tick pulse.

## C-04 — Fan-out Crossover

충분한 Load에서:

```text
direct latency
>
NOT → NOT으로 net 분할 latency
```

인 구간이 존재한다.

## C-05 — Feedback Ring

홀수 NOT, total loop delay D.

기대: stable condition에서 period 약 `2D`, Event 삭제 없음.

## C-06 — Symmetric Latch Startup

완전 대칭, initial LOW.

기대: Engine이 임의 state를 선택하지 않음. Oscillation 또는 X 가능.

## C-07 — Sensing

Enemy count 0→3→0.

기대: `LOW→HIGH→LOW`만 출력.

## C-08 — Brownout

같은 Circuit에서 `ρ=1.0`과 `ρ=0.5`.

기대: 낮은 ρ에서 Delay 증가, Drive / Work / Movement 감소.

## C-09 — Wire Break

Signal Arrival 이동 중 path element 파괴.

기대: Arrival 폐기, Signal / Power / Sense / Track 단절.

## C-10 — Contact Energy Conservation

같은 weight 대상 2개.

기대: 동일 흡수, 총합 <= granted Energy.

## C-11 — Radiation Falloff

동일 Source, distance band 1과 3.

기대: band 1 scheduled Energy >= band 3, 총합 <= emission.

## C-12 — Radiation Geometry

동일 Energy, orientation 다른 Wire.

기대: 적어도 한 Cell 분포가 다르고 budget 보존.

## C-13 — Radiation Arrival Timing

```text
A emission @ t, delay 3
B emission @ t+1, delay 2
```

기대: 둘 다 t+3 도착 후 합산.

## C-14 — Mobile Junction

| L | R | 결과 |
|---|---|---|
| 0 | 0 | straight |
| 1 | 0 | left |
| 0 | 1 | right |
| 1 | 1 | reverse |
| X | 0 / 1 / X | stop |
| 0 / 1 | X | stop |

## C-15 — Simultaneous Destruction

A와 B가 같은 Tick에 서로 치명적 Energy 전달.

기대: 둘 다 현재 Tick 행동 완료, 다음 Phase 0 제거.

## C-16 — Replay Determinism

다른 FPS / CPU core / 실행 속도.

기대: 모든 Tick State Hash 동일.

## C-17 — Numeric Geometry

조건:

```text
(0,0) → (3,4) wu
```

기대: length 5 wu.

음수 좌표:

```text
x = -1 fixed unit
cellSize = 1 wu
```

기대: cellX = -1.

## C-18 — Topology Synchronization

현재 HIGH Driver에 새 Wire와 Sink 연결.

기대:

- 같은 Tick 즉시 HIGH 아님
- 새 route Wire Delay 뒤 current Revision 도착
- 그 전 Sink passive LOW

## C-19 — Stale Route Arrival

Old route의 Revision 3 Event가 늦게 도착하고 새 route sync Revision 4가 먼저 도착.

기대: Revision 3 폐기, Sink가 과거 값으로 돌아가지 않음.

## C-20 — Same-tick Command Ordering

같은 위치에 두 Place Command, ordinal 1과 2.

기대: ordinal 1 accepted, ordinal 2 deterministic rejection.

## C-21 — Capacity Accounting

조건:

- 10 wu Wire 한 개가 네 역할 수행
- 같은 Geometry를 4 Segment로 split
- Module 내부 2 wu Wire 추가

기대:

```text
multi-role usage = 10 NCU
split usage = 10 NCU
with internal wire = 12 NCU
```

## C-22 — Soft Overcapacity

조건:

```text
S = 100 NCU
U = 120 NCU
```

기대:

- Wire 삭제 없음
- Build rejection 없음
- E = 20 NCU
- Support Demand > 0
- U 증가 시 Demand 단조 증가

## C-23 — Relay Activation

Activation Work가 Tick t Phase 11에 threshold 도달.

기대:

- Tick t Capacity 미기여
- Tick t+1 Phase 0 ONLINE
- Tick t+1 Supported Capacity 증가

## C-24 — Relay Loss

ONLINE Relay가 파괴됨.

기대:

- 파괴 Tick 행동 완료
- 다음 Phase 0 Capacity contribution 제거
- Existing Wire 유지
- Overcapacity가 증가할 수 있음
- Site에 Reconstruction Site 존재

## C-25 — Laboratory Edit Equivalence

같은 paused state와 같은 Edit Command Log를:

- Laboratory Single Step
- Headless World Step

에서 실행.

기대: 같은 Canonical Hash.

---

# 29. Stage Gate Contract

## 29.1 Stage 0 필수

```text
C-01
C-02
C-03
C-05
C-06
C-14
C-16
C-17
C-18
C-19
C-20
C-25
```

기술 통과 외에 PRD의 Product Gate가 필요하다.

> 현재 Input만으로 해결할 수 없는 World 행동 때문에 State를 만들고 싶어지는가?

## 29.2 Stage 1 필수

```text
C-07
C-08
C-09
C-10
C-21
C-22
```

그리고 동일한 World Input에서 Brute / Computed Architecture를 여러 Physical Scale Profile로 비교한다.

최소 기록:

- Survival
- Total Wire Length / NCU
- Gate Count
- A/O/N consumed
- Power
- Construction Work
- Heat
- Response Latency

Crossover가 실제 Layout에서 관찰되지 않으면 Profile 계수만 억지로 조정해 PASS로 만들지 않는다. H2를 재검토한다.

## 29.3 Stage 2 필수

```text
C-23
C-24
```

그리고 다음을 플레이테스트한다.

- Relay 확보 동기
- Relay Loss Crisis
- Expansion vs Compression 경쟁

## 29.4 MVP 필수

Radiation, Thermal, Reconstruction, 4 Enemy pressure가 서로 다른 Architecture를 허용해야 한다.

---

# 30. 의도적으로 제외한 것

v1.0에는 다음이 없다.

- Regional Network Capacity
- Wire Material Resource
- Analog voltage solver
- Tri-state Primitive
- Wireless control
- Battery / Capacitor
- Signal Crosstalk
- Continuous Maxwell field
- Continuous phase / polarization
- Automatic pathfinding
- Automatic targeting
- Automatic repair planning
- Mobile traffic collision
- Partial Integrity performance degradation
- Random metastability breaker
- Module runtime black box
- Enemy-specific required Circuit
- Permanent Stat Meta Progression

---

# 31. 주요 리스크와 가드레일

## R1. Capacity가 모든 것을 지배할 위험

가드레일:

- Capacity는 Wire Length만 측정한다.
- Overcapacity는 직접 Stat penalty가 아니다.
- Gate, Power, Heat, Delay, Work, Exposure는 별도 축으로 유지한다.
- Stage 1에서 Gate Count와 A/O/N 비용도 함께 측정한다.

## R2. Soft Limit가 Hard Limit처럼 느껴질 위험

Overcapacity curve가 너무 가파르면 Build가 기술적으로 가능해도 사실상 금지와 같다.

가드레일:

- Early-scale에서 E=0 또는 작은 E가 정상적이어야 한다.
- Crossover 전 Brute Force가 충분히 유효해야 한다.
- coefficient는 Parameter Sweep과 playtest로 결정한다.

## R3. Relay가 Map Unlock Token이 될 위험

가드레일:

- Relay는 Wire를 삭제하거나 Territory를 직접 열지 않는다.
- Capacity만 제공한다.
- 확보 비용은 실제 Wire / Power / Defense / Repair다.
- Relay Loss는 Existing Network의 Overcapacity로 이어진다.

## R4. Relay Online과 Power의 자기참조

Relay Capacity가 Overcapacity를 줄여 Relay Upkeep를 살리고, Offline이면 다시 켜지기 어려운 hysteresis가 생길 수 있다.

이는 의도된 stateful infrastructure일 수 있으나 무한 Tick oscillation은 허용하지 않는다.

가드레일:

- Online state는 Phase 0에서만 바뀐다.
- Activation Work와 Hold Threshold를 분리한다.
- Offline Grace를 explicit balance value로 둔다.

## R5. Physical Scale가 H2 결과를 조작할 위험

가드레일:

- Profile을 Replay와 Module에 기록한다.
- 단일 숫자를 GO 조건으로 숨기지 않는다.
- Parameter Sweep 전체 결과를 보존한다.
- 현실적인 Layout에서 Crossover가 없으면 H2 실패다.

## R6. Wire multi-role가 무료 통합처럼 느껴질 위험

한 Body를 네 역할로 쓰면 Capacity는 한 번만 계산된다.

그러나 동일 Body 파괴 시 네 역할이 함께 끊기고, Power / Heat / Delay / Exposure를 공유한다.

효율과 단일 실패점이 교환된다.

## R7. Topology live-edit가 이해하기 어려울 위험

가드레일:

- 새 route는 current sample을 Delay 뒤 동기화한다.
- 기존 Event는 reroute하지 않는다.
- Driver Revision으로 stale arrival을 막는다.
- Laboratory와 World가 같은 법칙을 쓴다.

---

# 32. 최종 불변식

```text
같은 Primitive Layout
+ 같은 Initial State
+ 같은 Command Log
+ 같은 World Input
+ 같은 Semantics Version
+ 같은 Numeric Profile
+ 같은 Physical Scale Profile
+ 같은 Balance Profile

= 같은 Network Usage
= 같은 Waveform
= 같은 Movement
= 같은 Power
= 같은 Heat
= 같은 Radiation
= 같은 Damage
= 같은 World
```

Network Capacity도 이 등식을 우회하지 않는다.

```text
Wire를 더 깐다
→ 실제 Length 증가
→ Capacity Usage 증가
→ 필요하면 Overcapacity Support 증가
→ Power / Heat Margin 감소
```

계산으로 Wire를 줄이면 그 반대가 일어난다.

> **문명은 무너진다. 설계는 남는다.**
