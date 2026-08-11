# A/O/N — Technical Requirements Document

**Full Title:** AND OR NOT — Technical Requirements Document
**Short Title:** A/O/N TRD
**Version:** v1.0 Draft
**Companion Documents:** A/O/N PRD v1.0 GO / A/O/N Simulation Semantics Specification v1.0 Draft
**Normative Scope:** Stage 0 Emergence Probe / Stage 1 Capacity Economy Probe / Stage 2 Relay Expansion Probe / Emergent Defense MVP
**Implementation Baseline:** Rust 2024 / Bevy 0.19.x
**Status:** 구현 착수용 아키텍처 계약 재베이스 초안
**작성 기준일:** 2026-08-11

---

# 0. 문서의 목적과 권위

이 문서는 A/O/N의 제품 요구와 Simulation Semantics를 실제 Rust 코드, 자료구조, 실행 파이프라인, Host 경계, 테스트, 실험 하네스로 옮기는 구현 계약이다.

문서 간 책임은 다음과 같다.

```text
제품 목표·가설·검증 순서·범위·비목표
→ PRD

관찰 가능한 World / Circuit 상태와 상태 전이
→ Simulation Semantics Specification

자료구조·알고리즘·패키지·캐시·Host·테스트·성능 전략
→ TRD
```

충돌 시:

- 무엇을 검증해야 하는지는 PRD를 따른다.
- 같은 Layout과 Input이 어떤 결과를 만드는지는 SSS를 따른다.
- 그 결과를 어떤 코드 구조로 계산하는지는 이 문서가 결정한다.

구현 최적화가 다음 결과를 바꾸면 최적화가 아니라 Semantics 변경이다.

- Signal Waveform
- Gate / Wire Delay
- Driver Revision과 Topology Synchronization
- Glitch / Race / Hazard
- Network Capacity Usage
- Overcapacity Support Demand
- Relay Online / Offline 상태
- Power / Brownout
- Heat
- Movement
- Radiation Arrival
- Damage / Destruction
- Replay State Hash

## 0.1 구현 범위

이 문서는 다음을 구현 가능한 수준으로 구체화한다.

- Pure Rust Canonical Simulation Core
- Bevy Interactive Host
- Rust Headless Runner
- Versioned Simulation Contract와 Profile 검증
- Fixed-point Numeric / Geometry Runtime
- Custom SoA Canonical World
- Explicit 12-Phase Tick Engine
- Event-based Signal Runtime
- Driver Revision / TopologySyncArrival / Path Certificate
- Signal / Power / Track / Body Topology Compile
- Global Network Capacity Accounting
- Main Core / Relay Runtime
- Power / Brownout / Heat / Damage
- Sensing / Mobility / Construction / Contact Attack
- Radiation Runtime
- Replay / State Hash / Cross-host Verification
- Stage 0·1·2 실험 하네스와 Product Gate

## 0.2 구현 단계와 제품 단계

구현 순서는 제품의 위험 검증 순서와 일치해야 한다.

```text
Stage 0
Emergence Probe

    ↓ PASS

Stage 1
Capacity Economy Probe

    ↓ PASS

Stage 2
Relay Expansion Probe

    ↓ PASS

MVP
Emergent Defense Vertical Slice
```

Stage Gate를 통과하지 않은 상태에서 다음 단계의 전체 시스템을 선행 구현하지 않는다.

이 규칙은 일정 관리 편의를 위한 것이 아니다.

> 각 단계는 다음 단계보다 더 근본적인 제품 가정을 더 싼 비용으로 검증한다.

## 0.3 비목표

이 문서는 다음을 확정하지 않는다.

- 최종 아트 스타일
- 상용 UI와 튜토리얼
- 최종 World Scale
- Enemy와 Wave의 최종 밸런스
- A/O/N 공급 비율
- Regional Network Capacity
- Multiplayer / Networking
- Cloud Backend / Workshop / Leaderboard
- Post-MVP Web Alpha의 최종 배포 기술
- GPU Canonical Simulation
- 모든 Stage의 동시 구현

---

# 1. 아키텍처 한 줄

> **A/O/N Core는 Bevy로 만든 게임 로직이 아니라, Rust로 작성된 독립적인 결정론적 시뮬레이터다. Bevy는 그 시뮬레이터를 관찰하고 조작하는 첫 번째 Interactive Host다.**

```text
┌──────────────────────────────────────────────────┐
│                    aon-app                       │
│                Bevy Interactive Host             │
│                                                  │
│ Input / Camera / ASCII Grid / Picking / UI      │
│ Waveform / Inspector / Analyzer / Rendering     │
│                                                  │
│           Bevy ECS = Presentation State          │
└───────────────────────┬──────────────────────────┘
                        │ CommandBatch
                        │ RenderSnapshot / StepReport
                        ▼
┌──────────────────────────────────────────────────┐
│                    aon-sim                       │
│             Pure Rust Canonical Core             │
│                                                  │
│ SimulationContract / Profiles                    │
│ Custom SoA / Explicit 12-Phase Engine            │
│ Event Calendar / Compiled Topology               │
│ Signal / Capacity / Relay / Power / Heat         │
│ Mobility / Damage / Radiation / Replay           │
│                                                  │
│ Bevy dependency = ZERO                           │
│ Wall-clock dependency = ZERO                     │
└───────────────────────┬──────────────────────────┘
                        │
             ┌──────────┴───────────┐
             ▼                      ▼
┌──────────────────────┐  ┌────────────────────────┐
│     aon-headless     │  │ cargo test / bench     │
│ Replay / Conformance │  │ Property / Sweep / CI  │
│ Scenario / Sweep     │  │                        │
└──────────────────────┘  └────────────────────────┘
```

## 1.1 Core / Host 경계의 목적

이 경계는 단순한 코드 정리를 위한 것이 아니다.

다음 등식을 구조적으로 강제하기 위한 것이다.

```text
같은 Initial World
+ 같은 Command Log
+ 같은 Simulation Contract

Headless Host
= Bevy Host
= 서로 다른 FPS
= 서로 다른 CPU core count

→ 같은 Tick State Hash
```

## 1.2 Stage별 구조 확장

### Stage 0

```text
aon-sim
├─ Numeric / Geometry
├─ Identity
├─ Command
├─ Gate / Wire / Junction
├─ Signal Event Runtime
├─ Feedback
├─ Mobility
└─ Replay
```

### Stage 1

```text
aon-sim
├─ Main Core
├─ Global Capacity Accounting
├─ Sensing
├─ Power Region / Brownout
├─ Heat / Damage
├─ Contact Attack
└─ Construction Work
```

### Stage 2

```text
aon-sim
├─ Relay Site / Relay Structure
├─ Anchor Connectivity
├─ Activation / Upkeep / Restart
├─ Relay Destruction
└─ Relay Reconstruction Site
```

### MVP

```text
aon-sim
├─ Payload / Transfer
├─ Full Reconstruction Loop
├─ Quartz
├─ Radiation
├─ Four Enemy Pressures
├─ Module Blueprint Contract
└─ Expanded Laboratory
```

---

# 2. 핵심 기술 결정

## 2.1 언어와 Edition

Canonical Simulation과 첫 Host는 Rust를 사용한다.

```toml
edition = "2024"
```

실제 compiler toolchain은 `rust-toolchain.toml`로 고정한다.

Architecture 문서는 patch version을 규범으로 만들지 않는다. Bootstrap commit에서 검증된 compiler와 dependency version을 pin한다.

## 2.2 Game Host

첫 Interactive Host는 Bevy 0.19.x를 사용한다.

Bevy의 책임:

- Native Window
- Keyboard / Mouse Input
- Camera / Viewport
- ASCII-like Grid Presentation
- Waveform / Inspector / Analyzer UI
- Presentation Interpolation
- Host-side Command Collection
- Audio / Effect의 미래 확장

Bevy가 소유하지 않는 것:

- Canonical Entity Identity
- Canonical Tick
- Simulation Contract
- Gate / Wire State Transition
- Signal Event Ordering
- Capacity / Relay Truth
- Power / Heat / Damage Result
- Movement Result
- Replay Truth
- State Hash

## 2.3 Canonical Core

`aon-sim`은 pure Rust library crate다.

`aon-sim`은 다음에 의존해서는 안 된다.

```text
bevy
winit
wgpu
window API
GPU API
OS timer
wall-clock timestamp
rendering frame state
thread scheduling result
```

Stage 0에서는 다음을 적용한다.

```rust
#![forbid(unsafe_code)]
```

`unsafe`가 실제 병목을 해결하는 유일한 수단이라는 profiling 증거가 생기기 전까지 유지한다.

## 2.4 Canonical ECS 금지

Canonical Core는 Bevy ECS를 사용하지 않는다.

```text
Canonical Simulation
→ Custom SoA + Explicit Stores + Explicit Scheduler

Bevy ECS
→ Presentation / Host Integration
```

Bevy `Entity`는 Replay, Tie-break, Event, Module Provenance의 Identity가 아니다.

## 2.5 Single-thread Reference First

Canonical Core의 최초 정본 구현은 single-threaded다.

```text
Memory race freedom
!=
Simulation determinism
```

병렬화는 다음이 성립한 뒤에만 허용한다.

1. Single-thread golden replay가 존재한다.
2. Phase input snapshot이 immutable하다.
3. Worker output이 독립 buffer에 기록된다.
4. Output에 완전한 canonical key가 있다.
5. Stable sort + deterministic reduction을 수행한다.
6. 모든 Tick hash가 reference와 일치한다.

## 2.6 Physics Engine 미사용

Canonical World Physics는 별도 범용 Physics Engine에 위임하지 않는다.

A/O/N의 물리는 이미 다음 Semantic으로 정의되어 있다.

- Fixed-point Geometry
- Wire Capsule / Body
- Track Position
- Swept Collider
- Radiation Cell
- Thermal Exchange
- Explicit Damage Commit

Host는 Picking과 Preview를 위해 Bevy의 비정본 geometry helper를 사용할 수 있다. Canonical 결과에는 사용할 수 없다.

## 2.7 Simulation Rate

Reference Balance Profile의 Canonical Simulation은 20Hz다.

```text
Canonical Simulation: 20 ticks/sec
Presentation Target:   60 frames/sec 이상
```

핵심 불변식:

```text
Simulation Tick Rate != Render Frame Rate
```

`Simulation::step()` 한 번은 정확히 한 Canonical Tick이다.

## 2.8 Steady-state Allocation 목표

```text
Topology가 변하지 않는 steady-state Tick
→ heap allocation 0 목표
```

허용되는 대표 할당 시점:

- Topology Revision 변경
- Construction Activation
- Replay Load
- Scenario Load
- Profile Load
- Buffer Capacity 초과에 따른 deterministic growth

Buffer growth는 Semantics를 바꾸지 않으며 Telemetry에 기록한다.

---

# 3. 절대적 구현 불변식

1. Canonical 계산 Primitive는 `AND / OR / NOT`뿐이다.
2. Wire는 하나의 Physical Body다.
3. Wire Body는 Signal / Power / Sense / Track Surface를 공유한다.
4. Circuit 내부 Wire도 World Wire와 동일한 Length·Capacity·Delay 법칙을 따른다.
5. `RepairBot`, `Memory`, `FSM`, `Router`, `Multiplexer`, `CPU`를 Runtime Gameplay Type으로 만들지 않는다.
6. Host는 Canonical State를 직접 변경하지 않는다.
7. 모든 Player 변경은 Command Log를 통해 Phase 0에 적용한다.
8. Module은 Runtime Black Box가 아니다.
9. Module Placement는 실제 Primitive Construction Site로 flatten한다.
10. Canonical 계산에서 `f32` / `f64` 결과에 의존하지 않는다.
11. EntityId는 Run 안에서 재사용하지 않는다.
12. 모든 인과 경로는 최소 1 Tick의 양의 지연을 갖는다.
13. Feedback을 same-tick fixed-point solver로 안정화하지 않는다.
14. Signal Event를 평균화하거나 생략하지 않는다.
15. Capacity는 Build Permission이 아니다.
16. Capacity Usage는 Active Physical Wire Length에서 파생한다.
17. Relay는 Capacity 외의 계산·라우팅·버퍼링 기능을 갖지 않는다.
18. Cache는 Canonical Truth가 아니다.
19. Cache를 삭제하고 재구축해도 같은 결과가 나와야 한다.
20. 동일한 Simulation Contract와 Input은 동일한 Tick Hash를 만든다.

## 3.1 Host 직접 변경 금지

```text
Host Input
→ CommandEnvelope
→ CommandBatch
→ Simulation::step()
→ Phase 0 Structural Commit
```

다음 API는 외부에 노출하지 않는다.

```rust
fn gates_mut(&mut self) -> &mut GateStore;
fn wires_mut(&mut self) -> &mut WireStore;
fn relay_mut(&mut self) -> &mut RelaySiteStore;
```

## 3.2 Derived State 원칙

다음은 Active Canonical Entity에서 재계산 가능한 Derived State다.

- Used Network Capacity
- Supported Network Capacity
- Overcapacity Excess
- Overcapacity Support Demand
- Power Region Membership
- Compiled Route
- Relay Anchor Connectivity
- Analyzer Classification

Derived State를 별도의 독립 Truth로 저장해서는 안 된다.

Performance를 위해 cache할 수 있으나:

- revision으로 invalidation하고
- State Hash에서 제외하며
- clear/rebuild equivalence test를 통과해야 한다.

---

# 4. Simulation Contract와 Profile

A/O/N의 한 Run은 네 가지 versioned contract를 명시한다.

```rust
pub struct SimulationContract {
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
    pub balance_profile_hash: ProfileHash,
}
```

## 4.1 Profile Bundle

```rust
pub struct ProfileBundle {
    pub numeric: NumericProfile,
    pub physical_scale: PhysicalScaleProfile,
    pub balance: BalanceProfile,
}
```

`Simulation::new()`는 다음을 검증해야 한다.

1. 각 Profile schema version이 지원되는가.
2. Canonical encoder로 계산한 hash가 Contract와 같은가.
3. Profile 내부 invariant가 유효한가.
4. Initial World geometry가 Physical Scale Profile에 맞는가.
5. Scenario가 요구하는 Stage feature가 활성화되어 있는가.

불일치 시 Simulation을 시작하지 않는다.

## 4.2 Semantics Version

다음이 바뀌면 `semantics_version`을 올린다.

- Tick Phase Ordering
- Truth Table
- Gate Inertial / Wire Transport
- Wire Length Accounting
- Overcapacity 함수 형태
- Relay 상태 전이
- Damage Commit 순서
- Radiation Allocation
- Deterministic Tie-break

TRD의 자료구조나 Cache 전략만 바뀌고 결과가 같으면 Semantics Version은 유지한다.

## 4.3 Numeric Profile

Numeric Profile은 다음을 포함한다.

```rust
pub struct NumericProfile {
    pub fixed_one: i64,
    pub overflow_policy: OverflowPolicy,
    pub division_profile: DivisionProfile,
    pub geometry_length_profile: GeometryLengthProfile,
}
```

v1 정본:

```text
FIXED_ONE = 65,536
coordinate floor = mathematical floor
segment length = ceil integer Euclidean sqrt
fixed coefficient rounding = nearest, ties to even
overflow = deterministic error
```

## 4.4 Physical Scale Profile

```rust
pub struct PhysicalScaleProfile {
    pub wire_geometry_quantum: Fixed,
    pub circuit_routing_pitch: Fixed,
    pub world_routing_pitch: Fixed,
    pub wire_body_radius: Fixed,
    pub gate_footprints: GateFootprintTable,
    pub gate_port_anchors: GatePortTable,
    pub substrate_clearance: Fixed,
}
```

Reference Stage 0 Profile:

```text
wireGeometryQuantum = 1/64 wu
circuitRoutingPitch = 1/4 wu
worldRoutingPitch   = 1 wu
wireBodyRadius      = 1/32 wu
gate minimum box    = 1/2 wu × 1/2 wu
```

## 4.5 Balance Profile

Balance Profile은 공식의 계수를 저장한다.

대표 범위:

- Simulation Hz
- Gate / Wire Delay
- Fan-out
- Drive Threshold
- Power / Heat
- Main Core Capacity
- Relay Capacity
- Overcapacity Coefficient
- Relay Activation / Upkeep
- Construction Work
- Sensing Radius
- Movement
- Radiation Kernel
- Damage Tolerance
- Quartz Period

Formula의 형태가 바뀌면 Semantics Version 변경이다. 계수만 바뀌면 Balance Profile Hash만 바뀔 수 있다.

## 4.6 Profile Canonical Hash

Profile hash는 파일 원문 bytes가 아니라 canonical field encoder의 결과로 계산한다.

규칙:

- fixed field order
- fixed-width integer encoding
- UTF-8 version string length-prefix
- map 사용 시 key sort
- comment / whitespace 제외
- unsupported field rejection

권장 hash algorithm:

```text
BLAKE3
algorithm id = "blake3-v1"
```

Hash algorithm id는 Replay와 Module Header에 기록한다.

Profile artifact의 coefficient는 JSON floating-point number로 저장하지 않는다. 다음 중 하나를 사용한다.

```text
fixed raw integer
또는
explicit numerator / denominator integer pair
```

문서의 `0.10`, `0.20` 표기는 사람용 표현이며 Canonical Profile은 정수 표현을 사용한다.

## 4.7 Module Compatibility

Module Blueprint는 최소 다음을 저장한다.

```text
semanticsVersion
numericProfileHash
physicalScaleProfileHash
absolute fixed-point geometry
```

다른 Physical Scale Profile에서 Module을 암묵적으로 resize하지 않는다.

처분은 둘뿐이다.

```text
exact compatibility
→ 그대로 사용

incompatible
→ explicit migration으로 새 Module 생성
```

## 4.8 Reference Profile Artifact

Bootstrap 시 다음 Profile fixture를 저장한다.

```text
profiles/numeric/v1.json
profiles/physical-scale/stage0-alpha.json
profiles/balance/stage0-alpha.json
profiles/balance/capacity-probe-alpha.json
profiles/balance/radiation-reference-alpha.json
```

### Numeric v1

```text
fixedScale = 65,536
coordinate floor = mathematical floor
segment length = ceil integer Euclidean sqrt
coefficient rounding = nearest, ties to even
overflow = deterministic error
```

### Stage 0 Physical Scale Alpha

| Parameter | Value |
|---|---:|
| wireGeometryQuantum | 1/64 wu |
| circuitRoutingPitch | 1/4 wu |
| worldRoutingPitch | 1 wu |
| wireBodyRadius | 1/32 wu |
| minimum Gate box | 1/2 × 1/2 wu |

### Stage 0 Balance Alpha

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

### Capacity Probe Alpha

| Parameter | Initial Probe Value |
|---|---:|
| Main Core Capacity | 1000 NCU |
| Relay Capacity | 500 NCU |
| overcapLinearK | 1.0 |
| overcapQuadraticK | 2.0 |
| capacityDenominatorFloor | 1 NCU |
| relayOfflineGraceTicks | 1 |
| supportHeatFraction | `0 < value <= 1` |

다음은 Parameter Sweep 대상이며 초기값을 제품 정답으로 취급하지 않는다.

- supportPowerPerNCU
- Relay Activation Work
- Relay Upkeep / Hold Threshold
- Gate Footprint
- Circuit / World Routing Pitch

### Radiation Reference Table

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

Orientation bin boundary는 fixed-point integer vector condition으로 Profile에 저장한다.

---

# 5. Cargo Workspace 구조

```text
and-or-not/
├─ Cargo.toml
├─ Cargo.lock
├─ rust-toolchain.toml
│
├─ crates/
│  └─ aon-sim/
│     ├─ Cargo.toml
│     └─ src/
│        ├─ lib.rs
│        ├─ contract.rs
│        ├─ profile/
│        ├─ numeric.rs
│        ├─ geometry/
│        ├─ identity.rs
│        ├─ command.rs
│        ├─ error.rs
│        ├─ world.rs
│        ├─ phase.rs
│        ├─ event/
│        ├─ topology/
│        ├─ logic/
│        ├─ signal/
│        ├─ capacity/
│        ├─ relay/
│        ├─ power/
│        ├─ sensing/
│        ├─ mobility/
│        ├─ construction/
│        ├─ thermal/
│        ├─ damage/
│        ├─ radiation/
│        ├─ replay/
│        ├─ snapshot/
│        └─ analyzer/
│
├─ apps/
│  ├─ aon-app/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     ├─ main.rs
│  │     ├─ plugin.rs
│  │     ├─ input/
│  │     ├─ presenter/
│  │     ├─ waveform/
│  │     ├─ inspector/
│  │     └─ analyzer/
│  │
│  └─ aon-headless/
│     ├─ Cargo.toml
│     └─ src/
│        ├─ main.rs
│        ├─ replay.rs
│        ├─ conformance.rs
│        ├─ scenario.rs
│        └─ sweep.rs
│
├─ profiles/
│  ├─ numeric/
│  ├─ physical-scale/
│  └─ balance/
│
├─ fixtures/
│  ├─ conformance/
│  ├─ scenarios/
│  ├─ replays/
│  └─ modules/
│
├─ experiments/
│  ├─ stage-1/
│  └─ stage-2/
│
└─ benches/
```

초기 dependency graph:

```text
aon-app ───────────► aon-sim

aon-headless ──────► aon-sim

integration tests ─► aon-sim
```

금지되는 역방향 dependency:

```text
aon-sim ─X─► aon-app

aon-sim ─X─► bevy
```

별도 `aon-common`, `aon-schema`, `aon-protocol` crate는 실제 독립 배포 또는 중복 제거 필요가 생길 때까지 만들지 않는다.

## 5.1 권장 초기 Dependency

`aon-sim`의 dependency는 최소화한다.

```text
serde       → versioned artifact serialization
thiserror   → deterministic error surface
blake3      → profile / state hash
```

Property test와 benchmark dependency는 dev-dependency로 격리한다.

정확한 version은 Cargo.lock에서 pin한다.

---

# 6. Public Core API

외부 API는 작고 굵게 유지한다.

```rust
pub struct Simulation {
    // private canonical world
    // reconstructible cache
    // reusable tick scratch
}

pub struct SimulationPackage {
    pub initial_world: InitialWorld,
    pub contract: SimulationContract,
    pub profiles: ProfileBundle,
}

impl Simulation {
    pub fn new(package: SimulationPackage)
        -> Result<Self, SimulationError>;

    pub fn step(
        &mut self,
        commands: &[CommandEnvelope],
    ) -> Result<StepReport, SimulationError>;

    pub fn write_render_snapshot(
        &self,
        out: &mut RenderSnapshot,
    );

    pub fn write_analyzer_snapshot(
        &self,
        out: &mut AnalyzerSnapshot,
    );

    pub fn state_hash(&self) -> StateHash;

    pub fn replay_header(&self) -> ReplayHeader;

    pub fn contract(&self) -> &SimulationContract;

    pub fn next_tick(&self) -> Tick;
}
```

원칙:

- `step()` 한 번은 정확히 한 Tick이다.
- Host가 Store reference를 얻지 않는다.
- Snapshot은 read-only projection이다.
- Debug / Analyzer API는 상태를 변경하지 않는다.
- Laboratory Reset은 기존 Simulation mutation이 아니라 새 `Simulation::new()`다.

## 6.1 Stage Feature 선언

Scenario는 필요한 Stage feature를 명시할 수 있다.

```rust
pub struct StageFeatureSet {
    pub signal: bool,
    pub mobility: bool,
    pub capacity: bool,
    pub sensing: bool,
    pub power: bool,
    pub relay: bool,
    pub payload: bool,
    pub radiation: bool,
}
```

이 값은 미구현 Semantics를 임의로 변경하는 toggle이 아니다.

- Stage 0 Scenario는 Capacity Store를 비워둘 수 있다.
- Stage 1 Scenario는 Relay Store를 비워둘 수 있다.
- 활성화된 feature의 Semantics는 항상 동일하다.

---

# 7. Canonical Numeric와 Geometry Runtime

## 7.1 Strong Type

```rust
#[repr(transparent)]
pub struct Tick(pub u64);

#[repr(transparent)]
pub struct Revision(pub u64);

#[repr(transparent)]
pub struct EntityId(pub u64);

#[repr(transparent)]
pub struct Fixed(pub i64);

#[repr(transparent)]
pub struct Energy(pub u64);

#[repr(transparent)]
pub struct HeatEnergy(pub u64);

#[repr(transparent)]
pub struct Integrity(pub u64);

#[repr(transparent)]
pub struct DriveStrength(pub u64);

#[repr(transparent)]
pub struct Capacity(pub u64);
```

의미가 다른 수치를 raw integer로 섞지 않는다.

```rust
impl Energy {
    pub fn checked_add(self, rhs: Self)
        -> Result<Self, NumericError>;
}
```

## 7.2 Fixed-point 상수

```rust
pub const FIXED_ONE: i64 = 65_536;
```

Profile 값과 compile-time implementation이 다르면 Profile Validation에서 실패한다.

## 7.3 Canonical Division Helper

최소 helper:

```rust
pub fn floor_div(n: i128, d: i128)
    -> Result<i128, NumericError>;

pub fn ceil_div_nonnegative(n: u128, d: u128)
    -> Result<u128, NumericError>;

pub fn round_div_nearest_even(n: i128, d: i128)
    -> Result<i128, NumericError>;
```

규칙:

- `d > 0`을 검증한다.
- Rust 기본 signed truncation을 Semantic 결과에 직접 사용하지 않는다.
- 각 formula가 별도 rounding을 요구하면 formula-specific helper를 사용한다.

## 7.4 Intermediate Width

다음 계산은 최소 `u128` 또는 `i128` intermediate를 사용한다.

```text
Fixed × Fixed
Energy × Weight
Distance × Flow²
E² in Overcapacity Curve
Conductance × Temperature Difference
Coordinate square sum
```

## 7.5 Overflow

Canonical arithmetic는 wrapping을 사용하지 않는다.

```text
Overflow
→ SimulationError::NumericOverflow
→ Run / Replay deterministic stop
```

Saturating arithmetic는 SSS 또는 Profile이 명시한 accumulator에만 허용한다.

## 7.6 FixedVec2

```rust
pub struct FixedVec2 {
    pub x: Fixed,
    pub y: Fixed,
}
```

Canonical coordinate는 Host pixel 또는 Bevy transform을 저장하지 않는다.

## 7.7 Geometry Quantization

모든 배치 좌표는 `wire_geometry_quantum`의 정수배여야 한다.

```rust
fn validate_quantized(
    point: FixedVec2,
    quantum: Fixed,
) -> Result<(), GeometryError>;
```

Host가 더 미세한 좌표를 보내면 Phase 0에서 거부한다.

```text
silent rounding 금지
```

## 7.8 Euclidean Length

```text
dx = x2 - x1
dy = y2 - y1
segmentLength = ceil_isqrt(dx² + dy²)
```

```rust
pub fn ceil_isqrt(value: u128) -> u64;

pub fn segment_length(
    a: FixedVec2,
    b: FixedVec2,
) -> Result<Fixed, NumericError>;
```

Polyline Length:

```text
wireLength = Σ segmentLength
```

같은 함수 결과를 다음에 재사용한다.

- Network Capacity
- Signal Route Length
- Power Route Length
- Construction Work
- Physical Exposure

Subsystem별로 서로 다른 거리 구현을 만들지 않는다.

## 7.9 Cell Coordinate

```rust
pub fn cell_coordinate(
    coordinate: Fixed,
    cell_size: Fixed,
) -> Result<i64, NumericError>;
```

내부 구현은 mathematical floor를 사용한다.

음수 좌표도 동일하다.

## 7.10 Numeric Conformance

최소 검증:

- `(0,0) → (3,4)` 길이 = `5 wu`
- `-1 fixed unit / 1 wu cell` 결과 = cell `-1`
- ties-to-even 양수 / 음수 사례
- maximum safe square
- overflow deterministic error
- quantization 위반 command rejection

---

# 8. Canonical Identity와 Entity Lifetime

## 8.1 Stable EntityId

Canonical EntityId는 Run 안에서 단조 증가하며 재사용하지 않는다.

```text
destroyed Wire
+ same Geometry reconstruction
→ new EntityId
```

이유:

- Path Certificate Validity
- Driver / Event Trace
- Replay Diff
- Tie-break
- Reconstruction Provenance

## 8.2 Typed Canonical ID

외부 Event와 Store 간 혼동을 줄이기 위해 typed wrapper를 사용한다.

```rust
pub struct GateId(pub EntityId);
pub struct WireId(pub EntityId);
pub struct JunctionId(pub EntityId);
pub struct DriverId(pub EntityId);
pub struct SinkId(pub EntityId);
pub struct RelaySiteId(pub EntityId);
pub struct MobileId(pub EntityId);
```

Runtime type이 Gameplay 의미 Class를 뜻하는 것은 아니다.

`DriverId`, `SinkId`는 topology endpoint identity다.

## 8.3 Dense Index

SoA 접근에는 non-canonical dense index를 사용한다.

```rust
pub struct GateIndex(pub u32);
pub struct WireIndex(pub u32);
pub struct JunctionIndex(pub u32);
pub struct RelaySiteIndex(pub u32);
```

Dense index는 cache / storage location이다.

- Replay에 저장하지 않는다.
- Tie-break에 사용하지 않는다.
- Compaction 후 바뀔 수 있다.

## 8.4 Entity Registry

```rust
pub struct EntityRegistry {
    next_id: u64,
    locations: Vec<Option<EntityLocation>>,
}

pub enum EntityLocation {
    MainCore,
    RelaySite(RelaySiteIndex),
    Gate(GateIndex),
    Wire(WireIndex),
    Junction(JunctionIndex),
    FixedSubstrate(FixedSubstrateIndex),
    MobileSubstrate(MobileSubstrateIndex),
    PowerSource(PowerSourceIndex),
    Quartz(QuartzIndex),
    Deposit(DepositIndex),
    Enemy(EnemyIndex),
    ConstructionSite(ConstructionSiteIndex),
}
```

## 8.5 Connection Generation

Path Certificate는 EntityId만으로 충분하지 않다.

Wire / Junction의 connectivity가 바뀌면 connection generation을 증가시킨다.

```rust
pub struct ConnectionGeneration(pub u64);
```

대표 증가 조건:

- Wire 활성화 / 제거
- Junction incident set 변경
- Port Binding 변경
- Substrate support loss로 topology 제거

Geometry와 EntityId가 같아도 generation이 다르면 이전 path event는 무효다.

## 8.6 Removal과 Compaction

초기에는 tombstone 또는 deterministic swap-remove를 사용할 수 있다.

조건:

- Cross-reference는 Canonical ID를 사용한다.
- Serialization은 EntityId 오름차순이다.
- Reduction은 dense iteration order에 의존하지 않는다.
- Compaction 전후 replay 결과가 같다.

Endless Run에서 tombstone 증가가 병목이라는 증거가 생기면 Phase 0 deterministic compaction을 추가한다.


---

# 9. Canonical World Storage

## 9.1 Simulation Root

Canonical State와 reconstructible cache, Tick scratch를 분리한다.

```rust
pub struct Simulation {
    canonical: CanonicalWorld,
    cache: SimulationCache,
    scratch: TickScratch,
}
```

```rust
pub struct CanonicalWorld {
    pub tick: Tick,
    pub topology_revision: Revision,
    pub contract: SimulationContract,

    pub entities: EntityRegistry,

    pub main_core: MainCoreState,
    pub relay_sites: RelaySiteStore,

    pub gates: GateStore,
    pub wires: WireStore,
    pub junctions: JunctionStore,
    pub fixed_substrates: FixedSubstrateStore,
    pub mobile_substrates: MobileSubstrateStore,

    pub power_sources: PowerSourceStore,
    pub quartz_nodes: QuartzStore,
    pub deposits: DepositStore,
    pub enemies: EnemyStore,

    pub construction_sites: ConstructionSiteStore,
    pub thermal_cells: ThermalCellStore,

    pub driver_events: EventCalendar<DriverTransition>,
    pub signal_events: EventCalendar<SignalArrival>,
    pub radiation_events: EventCalendar<RadiationArrival>,
    pub pending_destructions: Vec<EntityId>,
    pub pending_relay_transitions: Vec<RelayTransition>,
    pub pending_activations: Vec<ConstructionActivation>,

    pub path_certificates: PathCertificateArena,
}
```

Stage에서 사용하지 않는 Store는 empty state로 존재할 수 있다.

미구현 시스템을 임의의 가짜 결과로 채우지 않는다.

## 9.2 Canonical / Cache / Scratch 구분

### Canonical

Replay와 State Hash에 포함한다.

- Active Entity State
- Pending Event
- Driver Revision
- Sink Slot Sample
- Relay Progress
- Construction Progress
- Thermal State
- Path Certificate
- Simulation Contract

### Reconstructible Cache

State Hash와 Replay에서 제외한다.

- Compiled Topology
- Route Cache
- Power Region Cache
- Track Adjacency Cache
- Relay Anchor Connectivity Cache
- Spatial Index
- Radiation Kernel Cache
- Analyzer Cache

### Tick Scratch

Tick 종료 후 폐기하거나 재사용한다.

- Immutable Snapshot Buffer
- Intent Buffer
- Demand Buffer
- Grant Buffer
- Trajectory Buffer
- Exposure Buffer
- Thermal Contribution Buffer
- Event Staging Buffer
- Stable Sort Workspace

## 9.3 MainCoreState

```rust
pub struct MainCoreState {
    pub id: EntityId,
    pub position: FixedVec2,
    pub anchor_node: TopologyNodeId,
    pub capacity: Capacity,
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
}
```

Main Core는 Power Source가 아니다.

## 9.4 RelaySiteStore

Relay Site와 Relay Structure를 같은 Entity로 혼동하지 않는다.

```rust
pub struct RelaySiteStore {
    pub site_ids: Vec<RelaySiteId>,
    pub positions: Vec<FixedVec2>,
    pub attachment_nodes: Vec<TopologyNodeId>,

    pub structure_ids: Vec<Option<EntityId>>,
    pub modes: Vec<RelayMode>,
    pub capacities: Vec<Capacity>,
    pub integrity: Vec<Integrity>,
    pub heat_energy: Vec<HeatEnergy>,

    pub activation_progress: Vec<Energy>,
    pub has_ever_been_online: Vec<bool>,
    pub unhealthy_ticks: Vec<u64>,
}

pub enum RelayMode {
    Offline,
    Online,
    Destroyed,
}
```

`Activating`은 Canonical Mode가 아니다.

`Offline && activation_progress > 0`에서 Analyzer가 derived label로 표시할 수 있다.

## 9.5 GateStore

```rust
pub struct GateStore {
    pub ids: Vec<GateId>,
    pub alive: Vec<bool>,
    pub gate_type: Vec<GateType>,

    pub input_a: Vec<SinkId>,
    pub input_b: Vec<Option<SinkId>>,
    pub output: Vec<DriverId>,
    pub power_attachment: Vec<TopologyNodeId>,

    pub current_output: Vec<LogicLevel>,
    pub desired_output: Vec<LogicLevel>,

    pub pending_generation: Vec<u32>,
    pub pending_due_tick: Vec<Option<Tick>>,
    pub unpowered_ticks: Vec<u64>,

    pub heat_energy: Vec<HeatEnergy>,
    pub integrity: Vec<Integrity>,
    pub substrate: Vec<Option<EntityId>>,
}
```

각 Gate를 heap object로 만들지 않는다.

## 9.6 WireStore

하나의 Wire Body와 네 Surface를 같은 dense index로 묶는다.

```rust
pub struct WireStore {
    pub body: WireBodySoA,
    pub signal: WireSignalSoA,
    pub power: WirePowerSoA,
    pub sense: WireSenseSoA,
    pub track: WireTrackSoA,
}
```

```rust
pub struct WireBodySoA {
    pub ids: Vec<WireId>,
    pub alive: Vec<bool>,
    pub endpoint_a: Vec<TopologyNodeId>,
    pub endpoint_b: Vec<TopologyNodeId>,
    pub polyline_range: Vec<Range<u32>>,
    pub length: Vec<Fixed>,
    pub routing_domain: Vec<RoutingDomain>,
    pub substrate: Vec<Option<EntityId>>,
    pub connection_generation: Vec<ConnectionGeneration>,
    pub heat_energy: Vec<HeatEnergy>,
    pub integrity: Vec<Integrity>,
}
```

```rust
pub struct WireSignalSoA {
    pub active_signed_drive: Vec<i64>,
    pub previous_signed_drive: Vec<i64>,
}

pub struct WirePowerSoA {
    // Canonical per-body Power 값은 필요할 때 추가한다.
    // Region membership과 source route는 SimulationCache가 소유한다.
}

pub struct WireSenseSoA {
    pub sense_driver_a: Vec<DriverId>,
    pub sense_driver_b: Vec<DriverId>,
    pub sampled_presence: Vec<LogicLevel>,
}

pub struct WireTrackSoA {
    // Track edge / adjacency index는 SimulationCache가 소유한다.
}
```

Active Wire Body에는 Signal / Power / Sense / Track Surface가 모두 존재한다.

Surface를 선택적으로 제거하는 Player 기능은 v1.0에 없다. Operational 결과는 연결, Power, Signal, Damage에서 파생한다.

Capacity는 역할 수가 아니라 Body Length를 한 번만 계산한다.

## 9.7 Geometry Arena

```rust
pub struct GeometryArena {
    pub points: Vec<FixedVec2>,
}
```

Wire는 `Range<u32>`만 가진다.

다음을 피한다.

- Wire별 별도 `Vec`
- pointer chasing
- route compile 중 작은 allocation

## 9.8 JunctionStore

```rust
pub struct JunctionStore {
    pub ids: Vec<JunctionId>,
    pub alive: Vec<bool>,
    pub position: Vec<FixedVec2>,
    pub incident_range: Vec<Range<u32>>,
    pub connection_generation: Vec<ConnectionGeneration>,
    pub integrity: Vec<Integrity>,
    pub heat_energy: Vec<HeatEnergy>,
}
```

Incident Wire ID는 별도 contiguous arena에 저장한다.

## 9.9 Driver / Sink Store

```rust
pub struct DriverStore {
    pub ids: Vec<DriverId>,
    pub owner: Vec<EntityId>,
    pub sample: Vec<DriverSample>,
}

pub struct SinkStore {
    pub ids: Vec<SinkId>,
    pub owner: Vec<EntityId>,
    pub resolved_level: Vec<LogicLevel>,
    pub slot_ranges: Vec<Range<u32>>,
    pub dirty: Vec<bool>,
}

pub struct SinkDriverSlotStore {
    pub driver_id: Vec<DriverId>,
    pub sink_id: Vec<SinkId>,
    pub level: Vec<LogicLevel>,
    pub strength: Vec<DriveStrength>,
    pub revision: Vec<Revision>,
    pub emitted_at: Vec<Tick>,
}
```

Sink slot identity는 `(DriverId, SinkId)`다.

Route가 끊기면 해당 slot을 제거한다.

## 9.10 FixedSubstrateStore

```rust
pub struct FixedSubstrateStore {
    pub ids: Vec<EntityId>,
    pub alive: Vec<bool>,
    pub origins: Vec<FixedVec2>,
    pub routing_areas: Vec<FixedAabb>,
    pub footprints: Vec<FixedAabb>,
    pub integrity: Vec<Integrity>,
    pub heat_energy: Vec<HeatEnergy>,
}
```

Substrate는 계산하지 않는다. Circuit을 배치할 routing area와 physical support를 제공한다.

## 9.11 PowerSourceStore

```rust
pub struct PowerSourceStore {
    pub ids: Vec<EntityId>,
    pub alive: Vec<bool>,
    pub positions: Vec<FixedVec2>,
    pub power_attachments: Vec<TopologyNodeId>,
    pub generation_per_tick: Vec<Energy>,
    pub integrity: Vec<Integrity>,
    pub heat_energy: Vec<HeatEnergy>,
}
```

Main Core와 Relay는 이 Store에 암묵적으로 포함되지 않는다. Main Core는 Power Source가 아니며 Relay도 Power를 생성하지 않는다.

## 9.12 MobileSubstrateStore

```rust
pub struct MobileSubstrateStore {
    pub ids: Vec<MobileId>,
    pub alive: Vec<bool>,
    pub track_position: Vec<TrackPosition>,
    pub footprint: Vec<FixedAabb>,
    pub mass: Vec<Fixed>,

    pub stop_sink: Vec<SinkId>,
    pub left_sink: Vec<SinkId>,
    pub right_sink: Vec<SinkId>,
    pub load_sink: Vec<SinkId>,
    pub unload_sink: Vec<SinkId>,
    pub build_sink: Vec<SinkId>,

    pub cargo_range: Vec<Range<u32>>,
    pub heat_energy: Vec<HeatEnergy>,
    pub integrity: Vec<Integrity>,
}
```

`MOVE_TO` 목적지나 pathfinding state는 없다.

## 9.13 ConstructionSiteStore

```rust
pub struct ConstructionSiteStore {
    pub ids: Vec<EntityId>,
    pub target_kind: Vec<ConstructionTargetKind>,
    pub exact_geometry: Vec<GeometryRef>,
    pub required_cargo_range: Vec<Range<u32>>,
    pub supplied_cargo_range: Vec<Range<u32>>,
    pub required_work: Vec<Energy>,
    pub completed_work: Vec<Energy>,
    pub activation_ready: Vec<bool>,
}
```

Construction 완료 전 Wire Site는 WireStore의 Active Wire가 아니다.

---

# 10. Physical Geometry와 Wire Body

## 10.1 Routing Domain

```rust
pub enum RoutingDomain {
    OpenWorld,
    FixedSubstrate(EntityId),
    MobileSubstrate(EntityId),
}
```

- Open World vertex는 `world_routing_pitch`에 정렬한다.
- Substrate 내부 vertex는 local `circuit_routing_pitch`에 정렬한다.
- 모든 최종 좌표는 같은 World fixed-point coordinate로 환산 가능해야 한다.

## 10.2 Gate Footprint와 Port Anchor

Gate 배치는 Profile의 footprint와 port anchor를 사용한다.

```rust
pub struct GateFootprint {
    pub width: Fixed,
    pub height: Fixed,
}

pub struct GatePortAnchors {
    pub input_a: FixedVec2,
    pub input_b: Option<FixedVec2>,
    pub output: FixedVec2,
    pub power: FixedVec2,
}
```

Host가 임의 anchor를 생성해서는 안 된다.

## 10.3 Wire Validation

Phase 0에서 최소 다음을 검증한다.

1. 모든 vertex가 geometry quantum에 정렬되는가.
2. Routing Domain의 pitch를 만족하는가.
3. Polyline segment 길이가 0이 아닌가.
4. Endpoint가 유효한 Port / Junction / free endpoint인가.
5. 양의 길이 구간이 기존 별도 Wire Body와 정확히 겹치지 않는가.
6. Parallel Wire centerline 간격이 해당 Routing Pitch 이상인가.
7. Substrate 내부 Wire가 routing area를 벗어나지 않는가.
8. Unsupported Substrate 위에 놓이지 않는가.
9. 동일 Command Batch의 앞선 accepted geometry와 충돌하지 않는가.

## 10.4 Crossing과 Junction

```text
두 Wire가 한 점에서 교차
+ Junction 없음
→ 연결되지 않음
```

교차 자체는 허용할 수 있으나 signal / power / track connectivity를 만들지 않는다.

양의 길이 구간 overlap은 invalid다.

## 10.5 Multi-role Body

동일 Physical Wire를 여러 역할로 사용하려면 하나의 Body의 Surface를 함께 사용한다.

```text
Signal + Power + Sense + Track
→ one Wire Body
→ one Length
→ one Failure Surface
```

별도 Wire Body를 정확히 겹쳐 Capacity를 우회할 수 없다.

## 10.6 Substrate Support

Gate와 Circuit Internal Wire는 Substrate의 physical support를 가진다.

Substrate가 파괴되면:

1. Phase 10에서 pending destruction 표시
2. 다음 Phase 0에서 Substrate 제거
3. 지지를 잃은 내부 Primitive도 제거 대상으로 처리
4. Topology Revision 증가
5. 해당 Path Certificate 무효화

## 10.7 Geometry Canonicalization

동일한 polyline에 불필요한 collinear vertex가 추가되어도 Length 결과는 같아야 한다.

Editor는 collinear vertex를 축약할 수 있으나, 축약은 explicit command 결과로 새로운 geometry를 만든다.

State Hash는 저장된 Canonical Geometry를 반영한다.

---

# 11. Port와 Topology Model

## 11.1 Gate가 Wire Entity를 직접 참조하지 않는다

다음 모델을 사용하지 않는다.

```rust
struct Gate {
    input_wire_a: EntityId,
    input_wire_b: EntityId,
    output_wire: EntityId,
}
```

이 구조는 다음을 충분히 표현하지 못한다.

- Multi-driver
- Fan-out
- Junction
- Sense Port
- Driver Revision
- Canonical Route
- Topology Sync

## 11.2 Logical Port Model

```text
Gate / Sense / Quartz Output
→ Driver
→ Physical Signal Topology
→ Canonical Route
→ Sink
→ Gate Input / Actuator Input
```

Driver 예:

- Gate Output
- Wire Sense Output A/B
- Quartz Output
- External Laboratory Driver

Sink 예:

- Gate Input A/B
- Mobile STOP / LEFT / RIGHT
- LOAD / UNLOAD / BUILD
- Future World Actuator Input

Gate는 Logic Port와 별도로 Power Attachment를 가진다. Power Attachment는 Signal Sink가 아니며 Power Graph에만 참여한다.

## 11.3 Topology Layers

Compiled Topology는 하나의 거대한 Graph가 아니라 목적별 graph view를 가진다.

```text
Body Connectivity
├─ Relay Anchor Connectivity
└─ Physical Reachability

Signal Graph
├─ Driver / Sink Route
└─ Fan-out / Load

Power Graph
├─ Power Region
└─ Load-to-Source Route

Track Graph
├─ Edge
└─ Junction Turn Ordering
```

한 Wire Body가 여러 Graph에 동시에 참여할 수 있다.

## 11.4 Connectivity 규칙

Junction은 incident Wire의 다음 Surface를 연결한다.

- Signal
- Power
- Track

Sense Output은 Segment-local이다.

Junction에서 자동 OR되지 않는다.

## 11.5 Topology Revision

다음이 발생하면 `topology_revision`을 증가시킨다.

- Active Wire 생성 / 제거
- Junction 생성 / 제거
- Port Binding 변경
- Substrate support loss
- Main Core / Relay attachment connectivity 변경
- Surface connectivity를 바꾸는 structural command

단순 Signal Level 변화는 Topology Revision을 증가시키지 않는다.

---

# 12. Compiled Topology

## 12.1 Cache 성격

```rust
pub struct SimulationCache {
    pub compiled_topology: CompiledTopology,
    pub spatial_index: SpatialIndex,
    pub radiation_kernel_cache: RadiationKernelCache,
    pub analyzer_cache: AnalyzerCache,
}
```

Cache key:

```text
canonical.topology_revision
+ relevant profile hash
```

Cache rebuild 결과가 State Hash에 들어가지 않는다.

## 12.2 초기 Compile 전략

Stage 0~2 초기 구현은 full rebuild를 사용한다.

```text
Topology Revision changed
→ all graph views rebuild
```

최적화 순서:

```text
Full Rebuild
→ Revision Cache
→ Dirty Component Rebuild
→ Incremental Compile
```

Stage 0부터 incremental compiler를 만들지 않는다.

## 12.3 Body Connectivity Compile

목적:

- Main Core Anchor Reachability
- Relay Anchor Connectivity
- Physical Body Component
- Reconstruction diagnostics

정렬된 adjacency를 생성한다.

```text
TopologyNodeId
→ incident EntityId ascending
```

## 12.4 Signal Compile

절차:

1. Wire / Junction / Port adjacency graph 생성
2. Signal connected component 식별
3. Net별 Driver와 Sink 수집
4. Driver별 deterministic shortest route 계산
5. Driver-to-Sink Route compile
6. Route Length / Delay 계산
7. Driver reachable sink count 계산
8. Total connected wire length 계산
9. Sink slot range 생성
10. 이전 topology와 route diff 생성

## 12.5 Deterministic Signal Route

우선순위:

```text
1. 총 Euclidean Wire Length
2. Segment 수
3. ordered EntityId lexicographic order
```

Dijkstra 또는 동등한 알고리즘을 사용할 수 있다.

Priority key는 완전히 순서화되어야 한다.

```rust
pub struct RoutePriority {
    pub total_length: Fixed,
    pub segment_count: u32,
    pub path_key: PathLexicographicKey,
}
```

`BinaryHeap` insertion order와 adjacency storage order가 결과를 바꾸어서는 안 된다.

## 12.6 Power Compile

절차:

1. Power-enabled Wire / Junction / Device component 식별
2. Power Region 생성
3. Region별 Power Source 수집
4. 각 Load attachment에서 canonical source route 계산
5. Route Distance / Segment sequence 저장
6. Transmission Heat distribution range 생성

우선순위는 Signal Route와 같다.

## 12.7 Track Compile

절차:

1. Track-enabled Wire를 Edge로 compile
2. Junction별 incoming / outgoing adjacency 생성
3. Fixed-point vector로 turn ordering 계산
4. Geometry tie 시 EntityId로 정렬
5. Dead-end 정보 생성

Runtime `atan2`를 Canonical 결과에 사용하지 않는다.

## 12.8 Relay Anchor Connectivity

Relay attachment에서 Main Core anchor까지 Body Connectivity path가 존재하는지 compile한다.

이 cache는 Relay가 Signal을 중계한다는 뜻이 아니다.

```rust
pub struct RelayAnchorStatus {
    pub relay_site: RelaySiteId,
    pub connected: bool,
    pub path: Option<BodyPathId>,
}
```

## 12.9 Route Diff

Topology rebuild 전후 `(DriverId, SinkId)` route set을 비교한다.

결과:

```rust
pub struct RouteDiff {
    pub removed: Vec<DriverSinkPair>,
    pub added: Vec<CompiledRouteId>,
    pub retained: Vec<CompiledRouteId>,
}
```

- Removed route는 Sink Slot을 즉시 제거한다.
- Added route는 TopologySyncArrival을 예약한다.
- Retained route의 in-flight event due tick은 바꾸지 않는다.

## 12.10 SCC Analysis

SCC는 다음에만 사용한다.

- Analyzer
- Feedback Loop 표시
- Reachability 진단
- Future dirty propagation optimization

SCC로 same-tick fixed-point solve를 하지 않는다.

---

# 13. Event Runtime

## 13.1 Event 종류

Stage 0:

```text
DriverTransition
SignalArrival
TopologySyncArrival
```

Stage 1:

```text
ConstructionActivation
PendingDestruction
```

Stage 2:

```text
RelayTransition
```

MVP:

```text
RadiationArrival
```

`TopologySyncArrival`은 별도 enum variant이지만 Signal Arrival과 같은 sink application path를 사용한다.

## 13.2 Event Calendar Interface

```rust
pub trait EventQueue<T> {
    fn push(&mut self, event: T);
    fn drain_due(&mut self, tick: Tick, out: &mut Vec<T>);
    fn len(&self) -> usize;
}
```

초기 구현:

```text
pre-reserved BinaryHeap<Reverse<EventEntry<T>>>
```

이유:

- arbitrary future Tick
- contiguous internal storage
- 단순한 correctness baseline
- Timing Wheel로 교체 가능한 interface

Timing Wheel은 profiling으로 heap이 병목임이 확인될 때만 도입한다.

## 13.3 Canonical Event Key

```rust
pub struct EventKey {
    pub due_tick: Tick,
    pub kind_order: u8,
    pub target_id: u64,
    pub source_id: u64,
    pub revision: Revision,
    pub generation: u32,
    pub payload_order: u64,
}
```

같은 Tick Event는:

1. 모두 drain
2. canonical key stable sort
3. group apply

한다.

중요한 것은 순서대로 하나씩 World를 변경하는 것이 아니라, SSS의 simultaneous apply를 지키는 것이다.

## 13.4 DriverTransition

```rust
pub struct DriverTransition {
    pub key: EventKey,
    pub driver_id: DriverId,
    pub level: LogicLevel,
    pub strength: DriveStrength,
    pub pending_generation: u32,
    pub cause: DriverTransitionCause,
}
```

Cause 예:

- Gate Output
- Gate Strength Response
- Sense Sample
- Quartz Sample
- External Driver

Transition 적용 결과 Sample이 실제로 달라질 때만 Driver Revision을 증가시킨다.

## 13.5 SignalArrival

```rust
pub struct SignalArrival {
    pub key: EventKey,
    pub source_driver: DriverId,
    pub sink: SinkId,
    pub sample: DriverSample,
    pub path_certificate: PathCertificateId,
    pub kind: SignalArrivalKind,
}

pub enum SignalArrivalKind {
    Propagation,
    TopologySync,
}
```

## 13.6 RadiationArrival

```rust
pub struct RadiationArrival {
    pub key: EventKey,
    pub cell: CellCoord,
    pub energy: Energy,
    pub source_segment: WireId,
    pub emission_tick: Tick,
}
```

이미 방출된 Radiation Arrival은 Source Wire 파괴로 취소하지 않는다.

## 13.7 RelayTransition

```rust
pub struct RelayTransition {
    pub relay_site: RelaySiteId,
    pub due_tick: Tick,
    pub target_mode: RelayMode,
    pub cause: RelayTransitionCause,
}
```

Online / Offline / Destruction 결과는 Phase 0에서만 Structural State에 반영한다.

## 13.8 Event Staging

Phase 계산 중 Event Queue에 즉시 push하지 않는다.

```text
Phase calculation
→ staging buffer
→ canonical sort
→ queue append
```

이 규칙은 미래 deterministic parallelization의 경계를 만든다.

## 13.9 Inertial Cancellation

Heap 내부 Event를 임의 삭제하지 않는다.

Gate별 generation token을 사용한다.

```text
새 Transition 예약
→ pending_generation += 1
→ Event에 generation 저장

Input 변경 / 예약 취소
→ generation 증가

Event due
→ generation mismatch면 stale discard
```

Cancellation된 switching energy는 별도 Heat contribution으로 남긴다.

## 13.10 Event Serialization

Heap 내부 배열 순서를 직렬화하지 않는다.

Replay / State Hash에서는 모든 pending event를 `EventKey` 오름차순 view로 encode한다.

---

# 14. Driver Revision, Topology Sync, Path Certificate

## 14.1 Driver Sample

```rust
pub struct DriverSample {
    pub level: LogicLevel,
    pub strength: DriveStrength,
    pub revision: Revision,
    pub emitted_at: Tick,
    pub driver_id: DriverId,
}
```

Level 또는 Strength가 실제로 달라지면 Revision이 증가한다.

## 14.2 Driver Revision 규칙

```text
new level != old level
OR
new strength != old strength
→ revision += 1
```

같은 Sample을 중복 적용하면 Revision을 올리지 않는다.

## 14.3 Sink Slot Apply

Arrival을 Sink Slot에 적용할 때:

```text
incoming revision > slot revision
→ apply

incoming revision == slot revision
→ idempotent ignore 또는 동일값 검증

incoming revision < slot revision
→ stale discard
```

같은 Revision인데 다른 Sample이면 Canonical State invariant violation이다.

## 14.4 Topology Synchronization

Phase 0 rebuild 뒤 새로 생성된 route마다:

```text
현재 Driver Sample
+ 현재 Revision
+ 새 Route Delay
→ TopologySyncArrival
```

새 route가 같은 Tick에 Sink 값을 즉시 바꾸지 않는다.

최소 Wire Delay를 거친다.

## 14.5 Route Removal

Route가 제거되면:

1. `(Driver, Sink)` Slot 제거
2. Sink dirty 표시
3. Phase 2에서 Arrival이 없어도 resolve
4. 유효 Driver가 없으면 passive LOW

기존 in-flight Event는 path certificate validation 단계에서 폐기될 수 있다.

## 14.6 Path Certificate 구조

```rust
pub struct PathCertificateArena {
    pub certificates: Vec<PathCertificate>,
    pub elements: Vec<PathElementStamp>,
}

pub struct PathCertificate {
    pub id: PathCertificateId,
    pub element_range: Range<u32>,
}

pub enum PathElementStamp {
    Wire {
        id: WireId,
        generation: ConnectionGeneration,
    },
    Junction {
        id: JunctionId,
        generation: ConnectionGeneration,
    },
}
```

## 14.7 Scheduling 절차

1. Compiled Route에서 ordered path element를 읽는다.
2. Event staging canonical 순서로 Certificate를 생성한다.
3. Event에 Certificate ID를 저장한다.
4. Certificate와 element는 Canonical State에 포함한다.

## 14.8 Arrival Validation

도착 시 모든 element를 검증한다.

```text
Entity missing
OR
EntityId mismatch
OR
ConnectionGeneration mismatch
→ Arrival discard
```

같은 Geometry에 재건된 Wire는 이전 Event를 이어받지 않는다.

## 14.9 Event Reroute 금지

- 기존 Event는 새 경로로 reroute하지 않는다.
- 더 짧은 경로가 생겨도 due Tick을 바꾸지 않는다.
- 새 route는 current sample sync event를 별도로 가진다.

## 14.10 Certificate Lifetime

Stage 0에서는 consumed certificate tombstone을 허용한다.

장기 Run에서 메모리 병목이 확인되면:

1. pending Event reference count 계산
2. refcount 0 certificate 수집
3. Phase 0에서 ID 안정성을 깨지 않는 arena generation 또는 indirection table 사용
4. compaction 전후 State Hash equivalence 검증

Canonical Event가 Certificate ID를 참조하므로 단순 vector index 재배치는 금지한다.

---

# 15. Canonical Tick Engine

## 15.1 단일 Entry Point

```rust
pub fn step(
    &mut self,
    commands: &[CommandEnvelope],
) -> Result<StepReport, SimulationError> {
    let completed_tick = self.canonical.tick;

    self.phase_0_structural_commit(commands)?;
    self.phase_1_snapshot_and_world_sample()?;
    self.phase_2_driver_and_signal_arrival()?;
    self.phase_3_intent_evaluation()?;
    self.phase_4_global_accounting_and_nominal_demand()?;
    self.phase_5_power_solve_and_brownout()?;
    self.phase_6_scheduling_and_granted_work()?;
    self.phase_7_trajectory()?;
    self.phase_8_interaction()?;
    self.phase_9_thermal_integration()?;
    self.phase_10_damage_resolution()?;
    self.phase_11_progress_commit()?;

    self.canonical.tick = completed_tick.checked_next()?;
    let report = self.build_step_report(completed_tick)?;
    Ok(report)
}
```

`StepReport.completed_tick`은 Phase 11을 완료한 Tick이다.

`Simulation::next_tick()`은 다음에 실행할 Tick이다.

`StepReport.state_hash`는 Tick 증가까지 반영한 post-step Canonical State Hash이며,
반환 직후 `Simulation::state_hash()`와 같아야 한다.

## 15.2 Phase 0 — Structural Commit

구현 순서:

1. 이전 Tick pending destruction 적용
2. Reconstruction Site 생성
3. pending Relay transition 적용
4. 완료된 Construction Primitive 활성화
5. 현재 Tick Command를 ordinal 순서로 검증 / 적용
6. Module Placement flatten
7. connection generation 갱신
8. topology revision 증가 여부 확정
9. Topology full rebuild 또는 cache reuse
10. Removed route slot 제거
11. Added route TopologySyncArrival staging

Structural mutation이 발생하지 않으면 topology compile을 생략할 수 있다.

## 15.3 Phase 1 — Snapshot & World Sample

Tick 시작 상태를 immutable snapshot으로 고정한다.

대표 snapshot:

- Position
- Integrity
- Heat / Temperature
- Relay Mode
- Anchor Connectivity
- Online Capacity
- Hostile Occupancy Candidate
- Quartz Phase
- Enemy Intent Input
- Sink Resolved Signal
- Topology Revision

Phase 1 이후 Phase 11 전까지 snapshot source를 in-place 수정해 다른 Entity 계산에 영향을 주지 않는다.

## 15.4 Phase 2 — Driver / Signal Arrival

순서:

1. due DriverTransition drain
2. generation 검증
3. Driver Sample simultaneous apply
4. 실제 Sample 변화 Driver Revision 증가
5. 현재 compiled route로 Propagation Arrival staging
6. due SignalArrival drain
7. Path Certificate 검증
8. Sink Slot revision apply
9. dirty Sink 한 번 resolve

동일 Tick Event 배열 순서는 결과를 바꾸지 않는다.

## 15.5 Phase 3 — Intent Evaluation

Resolved Signal을 읽어 다음 Intent를 작성한다.

- Gate desired output
- Sense sample intent
- Quartz sample intent
- Live Wire intent
- Mobile STOP / LEFT / RIGHT
- LOAD / UNLOAD / BUILD
- Extraction
- Relay Activation / Upkeep
- Radiation emission
- Enemy attack

Intent는 아직 Work / Damage / Position을 변경하지 않는다.

## 15.6 Phase 4 — Global Accounting & Nominal Demand

먼저 Network Accounting을 계산한다.

```text
Used Capacity U
Supported Capacity S
Excess E
Total Support Demand
Wire별 Support Demand
```

그 뒤 모든 nominal demand를 수집한다.

- Gate idle / switching / drive
- Wire leakage
- Sensing
- Live Wire
- Overcapacity support
- Relay activation / upkeep
- Movement
- Extraction
- Transfer
- Construction
- Radiation emission

Demand iteration order가 allocation을 바꾸어서는 안 된다.

## 15.7 Phase 5 — Power Solve & Brownout

Power Region별 공통 `ρ`를 계산한다.

- 모든 Demand를 먼저 수집한다.
- Region Generation을 집계한다.
- deterministic integer binary search 또는 동등한 solver를 사용한다.
- Load별 granted power를 계산한다.
- source path transmission loss를 staging한다.

## 15.8 Phase 6 — Scheduling & Granted Work

- Gate Inertial Transition 예약 / 취소
- Driver Strength Transition `t + 1` 예약
- Sense / Quartz Driver Sample 예약
- Movement Budget 확정
- Live Wire Energy 확정
- Radiation Emission / Arrival 예약
- Transfer / Construction / Extraction Work 확정
- Relay Activation Work 확정
- Relay Upkeep health sample 확정

이미 예약된 Gate due Tick은 이후 Heat / Brownout 변화로 수정하지 않는다.

## 15.9 Phase 7 — Trajectory

Tick 시작 위치와 granted movement budget으로:

- Mobile trajectory
- Enemy trajectory
- Junction decision
- Dead-end reverse
- Swept collider

를 계산한다.

최종 Position은 Phase 11까지 commit하지 않는다.

## 15.10 Phase 8 — Interaction

1. due Radiation Arrival을 Cell별 합산
2. Contact Electrical Energy 누적
3. Radiation Absorption 누적
4. Enemy Attack Exposure 누적
5. Payload Transfer 누적
6. Construction / Extraction / Relay Work 누적
7. Movement / Switching / Support / Transmission Heat 누적

모든 contribution은 stable key로 reduction한다.

## 15.11 Phase 9 — Thermal Integration

Phase 9 시작 thermal state로 모든 exchange contribution을 계산한다.

- outgoing scaling
- stable remainder distribution
- simultaneous commit staging

을 수행한다.

## 15.12 Phase 10 — Damage Resolution

Electrical / Thermal Exposure를 Entity별로 합산한다.

- Integrity 감소
- pending destruction 표시
- Relay destruction 표시
- Main Core destruction 표시

현재 Tick 행동은 완료한다.

실제 제거는 다음 Tick Phase 0이다.

## 15.13 Phase 11 — Progress Commit

다음을 commit한다.

- Position
- Cargo
- Construction Work
- Extraction Result
- Relay Activation Progress
- Relay unhealthy tick
- Thermal State
- State Hash

Main Core가 파괴되었다면 Commit 뒤 Run을 종료한다.

## 15.14 Stage별 No-op 규칙

Stage에서 아직 존재하지 않는 Store는 명시적 no-op을 수행할 수 있다.

예:

```text
Stage 0
Capacity Store empty
Relay Store empty
Radiation Queue empty
```

단, Phase 자체를 삭제하거나 순서를 합치지 않는다.

## 15.15 Bevy Schedule과의 관계

Canonical Phase를 Bevy System으로 분해하지 않는다.

잘못된 구조:

```text
Bevy System phase_0
Bevy System phase_1
...
```

권장 구조:

```text
Bevy FixedUpdate
└─ advance_canonical_simulation
   └─ Simulation::step
      ├─ Phase 0
      ├─ Phase 1
      ├─ ...
      └─ Phase 11
```


---

# 16. Signal Runtime와 Feedback

## 16.1 Logic Domain

```rust
pub enum LogicLevel {
    Low,
    High,
    X,
}
```

`bool`을 사용하지 않는다.

## 16.2 Sink Resolution

Sink별로 Driver Slot을 연속 range로 보관한다.

Resolve 시 wide accumulator를 사용한다.

```text
H = HIGH strength sum
L = LOW strength sum
U = X strength sum
Θ = logic threshold
```

```text
U >= Θ                         → X
H >= Θ and L >= Θ              → X
H >= Θ                         → HIGH
L >= Θ                         → LOW
otherwise                      → LOW
```

HIGH / LOW 충돌 시:

```text
contention heat ∝ min(H, L)
```

같은 Tick의 Slot update를 모두 적용한 뒤 Sink를 한 번만 resolve한다.

## 16.3 Gate Truth Table

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

## 16.4 Startup

새 Gate 활성화 시:

```text
internal output = LOW
driver strength = 0
revision = initial revision
pending transition = none
```

Engine은 Feedback Circuit을 안정 상태로 자동 보정하지 않는다.

## 16.5 Effective Gate Delay

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

Load:

```text
inputLoad × reachableSinkCount
+
wireLoadPerLength × totalConnectedWireLength
```

Gate를 통과하면 새 Driver가 생기므로 load는 reset된다.

## 16.6 Inertial Delay

Phase 3에서 desired output을 계산한다.

Phase 6에서:

1. desired != current면 transition 예약
2. due 전 desired가 바뀌면 generation invalidation
3. desired가 current로 돌아오면 예약 취소
4. 예약 시점 effective delay 고정
5. 취소된 switching energy는 Heat로 남김

## 16.7 Wire Transport

Driver Sample이 실제로 바뀌면 현재 topology의 모든 compiled route에 Signal Arrival을 예약한다.

Wire는 Pulse를 삭제하지 않는다.

```text
Source 1 Tick Pulse
→ Wire Delay 5
→ Sink 1 Tick Pulse after 5 Tick
```

## 16.8 Driver Strength Response

Logic Level이 같더라도 Heat / Brownout으로 effective strength가 달라지면 새 Driver Sample이다.

Phase 6:

```text
active strength != next effective strength
→ DriverStrengthTransition due = t + 1
```

Strength 변화도 일반 Wire Delay 뒤 원격 Sink에 도착한다.

## 16.9 Power State Retention

Gate는 Power가 Logic 동작 임계값 아래인 Tick을 `unpowered_ticks`로 추적한다.

Balance Profile의 `gate_state_retention_ticks`가 장기 무전력 LOW 초기화의 경계를 제공한다.

구현 요구:

- Stage 0에서는 Power Ratio가 1.0이므로 이 경로를 사용하지 않는다.
- Stage 1 착수 전 retention boundary fixture를 추가한다.
- Retention 만료 전 Internal Output Level을 보존한다.
- Effective Driver Strength는 Power Grant에 따라 0으로 내려갈 수 있다.
- Retention 만료 후 LOW 초기화는 일반 Driver Transition / Revision / Wire Delay 경로를 우회하지 않는다.
- Exact threshold crossing Tick은 Balance Profile 값과 Tick-start counter 규칙으로 fixture에 고정한다.

## 16.10 Passive Default

유효 Driver Slot이 없는 Sink는 LOW다.

Route 제거 직후 Slot이 없어지면 Phase 2 resolve에서 LOW가 될 수 있다.

## 16.11 Independent Driver

모든 Gate Output은 독립 Driver다.

```text
loaded net
→ NOT
→ NOT
→ new net
```

별도 Buffer Primitive는 없다.

## 16.12 Feedback

Feedback route도 일반 Driver / Route / Arrival를 사용한다.

별도 Runtime Type을 만들지 않는다.

```text
Latch
Oscillator
Counter
Memory
FSM
```

은 관찰 가능한 Circuit 행동이지 Canonical Entity Type이 아니다.

## 16.13 Glitch / Race / Hazard

Path Delay 차이에서 나온 중간 Pulse를 제거하지 않는다.

다음에 연결되면 실제 World 행동이 된다.

- Live Wire
- Mobile Control
- BUILD
- Extraction
- Radiation
- Feedback State

Module 내부 조합회로를 memoize해 Event를 삭제하는 최적화는 금지한다.

## 16.14 Fan-out Crossover

Reference Profile은 충분한 load에서:

```text
direct long loaded net latency
>
NOT → NOT으로 net 분할 latency
```

인 영역을 만들어야 한다.

TRD는 해당 영역을 benchmark와 C-04 fixture로 검증한다.

---

# 17. Global Network Capacity Runtime

## 17.1 Capacity는 Derived Accounting이다

```rust
pub struct NetworkAccounting {
    pub used: Capacity,
    pub supported: Capacity,
    pub excess: Capacity,
    pub total_support_demand: Energy,
}
```

이 구조는 Phase 4 scratch / report다.

별도의 독립 Canonical Truth로 저장하지 않는다.

## 17.2 Capacity Unit

```text
1 NCU = 1 world unit의 Active Physical Wire centerline length
```

Canonical 내부에서는 NCU를 Fixed scale의 non-negative `Capacity`로 표현한다.

예:

```text
0.25 wu Active Wire
→ 0.25 NCU
```

Wire별로 정수 NCU로 반올림하지 않는다. Fixed Length를 합산한 뒤 UI에서만 표시 단위를 변환한다.

## 17.3 Used Capacity

```text
U = Σ active Wire Body length
```

포함:

- World Backbone
- Circuit Internal Wire
- Sensor Wire
- Track Wire
- Contact Attack Wire
- Radiation Wire
- Fixed / Mobile Substrate Internal Wire

제외:

- Construction 완료 전 Wire Site
- Phase 0에서 제거된 Wire
- Gate / Junction / Substrate Body

Phase 10에서 pending destruction이 된 Wire는 현재 Tick까지 Active다. 다음 Tick Phase 0에서 제거된 뒤 Usage에서 빠진다.

Reference implementation은 Phase 4에서 alive Wire Length를 EntityId stable order로 합산한다.

성능 최적화로 revision cache를 추가할 수 있으나 clear/recompute equivalence가 필요하다.

## 17.4 Multi-role Accounting

```text
one Wire Body
+ four enabled Surfaces
→ length charge once
```

Surface별 합산을 하지 않는다.

## 17.5 Length Preservation

같은 Geometry를 Junction에서 여러 Segment로 분할해도 총 Length가 같으면 Usage가 같아야 한다.

Capacity accumulator는 Wire별 rounding을 하지 않는다.

모든 Fixed Length를 합산한 뒤 UI에서만 표시 단위를 변환한다.

## 17.6 Supported Capacity

```text
S = Main Core Capacity
  + Σ Online Relay Capacity
```

Main Core가 살아 있는 Run에서 Core Capacity를 제공한다.

Relay는 Phase 0에서 `Online`인 경우에만 기여한다.

## 17.7 Excess

```text
E = max(0, U - S)
```

`E > 0`이어도:

- Wire Build를 거부하지 않는다.
- Existing Wire를 삭제하지 않는다.
- Signal Delay에 직접 penalty를 더하지 않는다.
- Capacity Damage Type을 만들지 않는다.

## 17.8 Support Curve

```text
D_support = supportPowerPerNCU
          × (
              overcapLinearK × E
              + overcapQuadraticK × E²
                / max(S, capacityDenominatorFloor)
            )
```

구현 요구:

- `u128` intermediate
- denominator 0 금지
- final demand `ceil_div_nonnegative`
- `D_support(0) = 0`
- E에 대해 단조 증가
- coefficient validation

```rust
pub fn calculate_support_demand(
    accounting: CapacityInputs,
    profile: &BalanceProfile,
) -> Result<Energy, NumericError>;
```

## 17.9 Wire별 Demand Distribution

`U > 0`일 때:

```text
D_support_e = D_support × length_e / U
```

구현 절차:

1. 각 Wire floor share 계산
2. remainder 합산
3. Wire EntityId 오름차순으로 1 Energy unit 배분
4. 배분 합이 Total Support Demand와 같은지 검증

각 Wire share는 그 Wire가 속한 Power Region의 intrinsic load다.

Global Excess는 모든 Active Wire Length를 사용하므로, abandoned Wire도 전체 Excess를 올리고 정상 Network Wire의 Support Demand를 증가시킬 수 있다.

## 17.10 Support Heat

Phase 8에서:

```text
granted Support Energy
× supportHeatFraction
→ target Wire Heat
```

나머지는 모델링하지 않는 유지 손실이다.

Profile Validation은 다음을 강제한다.

```text
0 < supportHeatFraction <= 1
supportPowerPerNCU > 0
overcapLinearK >= 0
overcapQuadraticK > 0
capacityDenominatorFloor > 0
```

Capacity 자체가 별도 timing penalty를 적용하지 않는다.

```text
Capacity
→ Power Demand
→ Brownout / Granted Energy
→ Heat
→ Timing / Damage
```

## 17.11 Relay Loss

Relay가 Phase 0에서 Offline / Destroyed가 되면 `S`가 즉시 감소한다.

Active Wire는 유지된다.

같은 Tick Phase 4에서 더 큰 Excess와 Support Demand가 계산된다.

## 17.12 Capacity Analyzer

최소 관찰 값:

- Used NCU
- Supported NCU
- Excess NCU
- Total Support Demand
- Wire별 Support Demand
- Region별 Support Load
- Support Heat
- Relay별 Capacity Contribution

## 17.13 Capacity Conformance

필수:

- Multi-role Wire one-time accounting
- Segment split length preservation
- Internal Circuit Wire 포함
- Overcapacity build rejection 없음
- Demand monotonicity
- Stable remainder
- Relay loss 후 Used 유지 / Supported 감소

---

# 18. Main Core와 Relay Runtime

## 18.1 Main Core

Main Core의 책임:

- Run 종료 조건
- Initial Global Capacity
- Network Anchor Root

Main Core가 하지 않는 것:

- Power Generation
- Automatic Routing
- Signal Processing
- Load Priority

## 18.2 Relay Site / Structure 분리

Relay Site는 immutable World location이다.

Relay Structure는 파괴 / 재건 가능한 body다.

```text
Site exists forever in Run
Structure may be Offline / Online / Destroyed
```

새 Relay Site 생성 Command는 없다.

## 18.3 Relay Mode

```rust
pub enum RelayMode {
    Offline,
    Online,
    Destroyed,
}
```

`Activating`은 derived UI state다.

## 18.4 Anchor Connectivity

Relay activation / upkeep에는 다음이 필요하다.

```text
Relay attachment
→ live Wire / Junction Body path
→ Main Core Anchor
```

Body Connectivity cache를 사용한다.

Signal route나 power processing을 Relay가 수행하는 것은 아니다.

## 18.5 Activation Intent

Offline Relay가 다음을 만족하면 Phase 3에서 activation intent를 제출한다.

- Structure intact
- Anchor connected
- Activation Power path 존재

Phase 4에서 nominal demand를 제출하고 Phase 6에서 granted work를 확정한다.

Phase 8에서 contribution을 누적하고 Phase 11에서 progress를 commit한다.

## 18.6 Activation Target

```text
hasEverBeenOnline == false
→ relayActivationWork

hasEverBeenOnline == true
→ relayRestartWork
```

Progress가 threshold를 넘으면:

```text
RelayTransition Online due = next Tick Phase 0
```

Online commit 시:

```text
activation_progress = 0
has_ever_been_online = true
unhealthy_ticks = 0
```

Threshold 도달 Tick에는 Capacity를 제공하지 않는다.

## 18.7 Progress Persistence

Initial activation 중 Power / Connection이 끊겨도 `activation_progress`는 유지한다.

Offline transition commit 시 `activation_progress = 0`으로 만들며 restart progress는 0에서 시작한다.

## 18.8 Online Upkeep

Online Relay는 intrinsic upkeep demand를 제출한다.

```text
healthy = anchorConnected
       and grantedUpkeep >= relayHoldThreshold
```

Phase 11:

```text
healthy
→ unhealthyTicks = 0

unhealthy == true
→ unhealthyTicks += 1
```

```text
unhealthyTicks >= relayOfflineGraceTicks
→ Offline transition next Phase 0
```

`relayOfflineGraceTicks >= 1`을 Profile Validation에서 강제한다.

## 18.9 Hysteresis

Activation Work와 Hold Threshold를 분리한다.

Relay Capacity가 Overcapacity를 줄이고 Upkeep를 살리는 feedback은 허용한다.

하지만 same-tick infinite toggle은 허용하지 않는다.

- Mode change는 Phase 0에서만 발생
- Health sample은 Phase 6
- unhealthy counter는 Phase 11
- transition은 next Tick

## 18.10 Relay Destruction

Phase 10에서 Integrity <= 0:

- pending destruction
- 현재 Tick 행동 완료

다음 Phase 0:

- Online contribution 제거
- Structure ID 제거
- Mode = Destroyed
- activation progress reset
- unhealthy counter reset
- Site에 Reconstruction Site 생성

Adjacent Wire는 자동 삭제하지 않는다.

## 18.11 Relay Reconstruction

Relay Site의 Reconstruction Site는 실제 Work / Power / Cargo contract를 사용한다.

완료 시 다음 Phase 0:

```text
new Relay Structure EntityId
Mode = Offline
activation progress = 0
```

새 Structure는 다시 activation해야 한다.

## 18.12 Relay Conformance

필수:

- Activation threshold Tick에는 Capacity 미기여
- next Phase 0 Online
- Upkeep grace
- Restart Work
- destruction next Phase 0 contribution removal
- existing Wire 유지
- Reconstruction Site 생성
- same Site new Structure ID

---

# 19. Power Network와 Brownout

## 19.1 Power는 Flow다

Power는 Inventory가 아니다.

각 Tick의 Generation / Demand / Granted Work로 계산한다.

## 19.2 Demand Record

```rust
pub struct PowerDemand {
    pub id: DemandId,
    pub owner: EntityId,
    pub kind: DemandKind,
    pub region: PowerRegionId,
    pub nominal: Energy,
    pub source_route: Option<PowerRouteId>,
}
```

대표 Kind:

- GateIdle
- GateSwitch
- GateDrive
- WireLeakage
- WireSensing
- LiveWire
- OvercapacitySupport
- RelayActivation
- RelayUpkeep
- Movement
- Extraction
- Transfer
- Construction
- RadiationEmission

## 19.3 Power Region

Power-enabled Wire / Junction / Device의 connected component다.

Power Region은 Compiled Cache다.

Region ID를 Canonical Truth로 직렬화하지 않는다.

## 19.4 Canonical Source Route

Load에서 Source까지 우선순위:

1. Euclidean Wire Length
2. Segment Count
3. EntityId Lexicographic Path

Region 내 Power Source Generation은 합산한다.

Source가 없으면:

```text
G = 0
ρ = 0
```

## 19.5 Baseline Leakage와 Switching Demand

Wire baseline leakage:

```text
wireLeakage = leakagePerLength
            × wireLength
            × leakageThermalFactor(T)
```

Signal switching demand는 switching activity와 connected load의 단조 함수다.

```text
signalPower ≈ switchingActivity × load
```

정확한 coefficient와 fixed-point rounding은 Balance Profile이 제공한다.

Leakage와 switching의 granted / lost Energy는 Phase 8 Heat Contribution으로 연결한다.

## 19.6 Transmission Loss

```text
P_i = ρ × D_i
```

```text
sourceCost_i = P_i
             + powerLossK × distance_i × P_i²
```

Power Ratio는 Fixed `0..FIXED_ONE` 범위로 표현한다.

## 19.7 Region Solver

Region Generation `G`에 대해 다음을 만족하는 최대 `ρ`를 찾는다.

```text
Σ sourceCost_i(ρ) <= G
```

초기 구현:

```text
fixed-point deterministic binary search
```

요구:

- search iteration count profile-independent fixed upper bound
- lower / upper bound deterministic
- rounding helper 명시
- all demand pre-collected
- iteration order independent

## 19.8 Common Ratio

같은 Region의 모든 Load는 같은 `ρ`를 받는다.

먼저 순회된 Load가 Power를 독점하지 않는다.

## 19.9 Granted Record

```rust
pub struct PowerGrant {
    pub demand_id: DemandId,
    pub granted: Energy,
    pub ratio: Fixed,
    pub transmission_loss: Energy,
}
```

Phase 6 / 8은 Grant를 읽는다.

## 19.10 Brownout Effect

`ρ`는:

- Gate Delay
- Gate Drive
- Sense Drive
- Live Wire Energy
- Radiation Energy
- Movement Speed
- Extraction / Construction Work
- Relay Activation / Upkeep

에 영향을 준다.

`ρ < logicOperateThreshold`이면 Logic Driver가 유효 Drive를 제공하지 못한다.

## 19.11 Load Shedding

Engine은 Priority Scheduler를 제공하지 않는다.

플레이어가 Circuit으로 Load를 비활성화하거나 Power Region을 분리한다.

Overcapacity Support와 Relay Upkeep는 intrinsic load다.

Signal Port로 직접 OFF할 수 없다.

## 19.12 Transmission Heat

Transmission Loss는 Route Wire에 Length 비례로 분배한다.

Remainder는 ordered Wire EntityId 순서로 배분한다.

Phase 8 Heat Contribution으로 들어간다.

---

# 20. Mobility Runtime

## 20.1 Track Position

```rust
pub enum TrackPosition {
    Edge {
        edge_id: WireId,
        offset: Fixed,
        heading: Heading,
    },
    Junction {
        junction_id: JunctionId,
        incoming_edge_id: WireId,
    },
}
```

자유 2D Transform을 Canonical Mobile Position으로 사용하지 않는다.

## 20.2 Intrinsic Port

```text
STOP
LEFT
RIGHT
LOAD
UNLOAD
BUILD
```

없음:

```text
MOVE_TO
PATHFIND
REPAIR
DELIVER_TO
```

## 20.3 Footprint

```text
mobileFootprint = boundingBox(internal geometry)
                + payload area
                + profile clearance
```

Footprint는:

- Collider
- Damage Exposure
- Construction Work
- Mass

에 사용한다.

## 20.4 Mass

Mass는 다음을 포함한다.

- Substrate Body
- Gate
- Internal Wire
- Payload A/O/N
- Other Cargo

```rust
pub struct MobileMassBreakdown {
    pub substrate: Fixed,
    pub gates: Fixed,
    pub wires: Fixed,
    pub cargo: Fixed,
}
```

## 20.5 Movement Budget

```text
movementBudget = baseMovePerTick
               × powerRatio
               ÷ massFactor(totalMass)
```

Stage 0에서는 `powerRatio = 1.0`으로 고정한다.

## 20.6 Junction Decision

Control은 Phase 3에서 한 번 sample한다.

`STOP = HIGH` 또는 필요한 control이 X면 정지한다.

| LEFT | RIGHT | 결과 |
|---|---|---|
| LOW | LOW | straight |
| HIGH | LOW | left |
| LOW | HIGH | right |
| HIGH | HIGH | reverse |

Tie-break:

1. turn angle ordering
2. EntityId

Degree 1 dead-end에서 `LOW / LOW`는 reverse한다.

## 20.7 Turn Ordering

Canonical Core는 fixed-point vector cross / dot comparison을 사용한다.

Runtime `atan2`와 platform-dependent trigonometry를 사용하지 않는다.

## 20.8 Power Boundary

다음 Track Segment가 무전력이면 경계에서 정지한다.

v1.0에는:

- Inertia
- Battery Coast

가 없다.

## 20.9 Multiple Mobile

Mobile끼리는 Track Capacity를 점유하지 않고 통과할 수 있다.

Traffic / Collision은 별도 Semantics revision이다.

## 20.10 Stage 0 Scenario

```text
A 출발
→ Junction
→ B 도착
→ 과거 조건 유지
→ STOP / RETURN
```

Core에 FSM / RoutePlanner Class 없이 Feedback Circuit과 저수준 Port로 동작해야 한다.

---

# 21. Sensing, Construction, Contact Runtime

## 21.1 Spatial Index

Sensing, Contact, Radiation Absorption broad phase는 공통 spatial infrastructure를 공유할 수 있다.

권장 초기 구조:

```text
Sparse Ordered Chunk Grid
→ Candidate Collect
→ EntityId Stable Sort
→ Deterministic Narrow Phase
```

HashMap bucket iteration을 결과 순서로 사용하지 않는다.

## 21.2 Wire Sensing

각 Wire straight segment의 capsule을 spatial index에 등록한다.

Phase 1:

```text
hostile collider intersects capsule
→ presence HIGH
else LOW
```

한 명과 여러 명을 구분하지 않는다.

Canonical Sense Output은 `LOW / HIGH` occupancy 한 값뿐이다. Enemy count, type, HP, velocity, target을 Driver payload에 넣지 않는다.

## 21.3 Sense Driver

Sense sample은 `senseDelay` 뒤 Driver Transition으로 전달한다.

Power Grant가 threshold 미달이면 effective strength가 낮아져 Sink에 passive LOW로 보일 수 있다.

별도 Sensor Health bit는 없다.

## 21.4 Construction Work

```text
Gate Work      = gateWorkByType
Junction Work  = junctionBaseWork
Wire Work      = wireEndpointWork
               + wireWorkPerNCU × wireLength
Substrate Work = substrateWorkPerArea × area
Relay Work     = relayReconstructionWork
```

긴 Wire는 더 많은 Work를 요구한다.

## 21.5 Construction Site Lifecycle

```text
Command accepted
→ Construction Site
→ Cargo / Work accumulation
→ activation_ready
→ next Phase 0 active Primitive
```

완료 전 Wire Site는:

- Signal 없음
- Power 없음
- Sense 없음
- Track 없음
- Capacity Usage 없음

## 21.6 Capacity와 Construction

Capacity 부족은 Construction 완료를 막지 않는다.

Wire가 Active가 된 Phase 0부터 전체 Length가 Usage에 포함된다.

## 21.7 Transfer

`LOAD` / `UNLOAD`가 HIGH이고 compatible endpoint와 겹치면 Transfer Intent를 제출한다.

여러 대상이 겹치면 EntityId가 작은 하나를 선택한다.

여러 Mobile이 같은 Inventory에 접근하면:

1. 모든 Intent 수집
2. Mobile EntityId 정렬
3. Available Unit 배정

## 21.8 Cargo Requirement

- AND Gate는 A cargo를 요구한다.
- OR Gate는 O cargo를 요구한다.
- NOT Gate는 N cargo를 요구한다.
- Wire / Junction / Substrate는 A/O/N cargo를 요구하지 않지만 Power와 Work를 요구한다.
- Relay Reconstruction Cargo는 Balance Profile이 정의한다. Reference Profile은 cargo 없이 Power와 Work만 요구한다.

Cargo requirement와 Work requirement를 모두 만족해야 activation-ready가 된다.

## 21.9 Contact Detection

Enemy / damageable swept collider가 actual Wire Body와 교차하면 Contact다.

Sense Radius가 아니라 `wire_body_radius`를 사용한다.

## 21.10 Live Wire Demand

```text
liveDemand = liveEnergyPerStrengthLength
           × highDriveStrength
           × segmentLength
```

일반 Logic Drive는 작은 공격 출력을 가진다.

여러 HIGH Driver는 Strength가 합산될 수 있다.

반대 Driver는 X와 contention heat를 만든다.

## 21.11 Contact Energy Allocation

```text
weight_i = contactDuration_i
         × conductivity_i
         × contactMeasure_i
```

```text
absorbed_i = liveEnergy
           × weight_i
           ÷ (worldLeakWeight + Σ weight)
```

Remainder는 EntityId 오름차순으로 배분한다.

```text
Σ absorbed <= granted live energy
```

나머지는 Wire Heat다.

## 21.12 Friendly Fire

Faction immunity는 없다.

정상 attach된 자기 Gate / Junction / Substrate는 contact target에서 제외한다.

외부에서 겹친 Player Entity는 피해를 받을 수 있다.

---

# 22. Thermal와 Damage Runtime

## 22.1 Thermal State

```text
Temperature = HeatEnergy / ThermalCapacity
```

Canonical State는 HeatEnergy를 저장한다.

Temperature는 profile capacity로 계산 가능한 derived value다.

## 22.2 Heat Source

- Gate idle / switching
- Cancelled switching
- Wire leakage
- Signal contention
- Transmission loss
- Overcapacity support
- Unused Live Energy
- Contact remainder
- Radiation inefficiency
- Movement
- Extraction
- Construction
- Enemy Thermal Attack

## 22.3 Thermal Edge

```rust
pub struct ThermalEdge {
    pub source: ThermalNodeId,
    pub destination: ThermalNodeId,
    pub conductance: Fixed,
    pub edge_id: u64,
}
```

## 22.4 Simultaneous Exchange

Phase 9 시작 상태로:

```text
q_ideal = conductance × abs(Ta - Tb)
```

높은 Temperature에서 낮은 쪽으로 흐른다.

한 Source의 total outgoing이 available Heat를 넘으면:

```text
q_granted_e = q_ideal_e
            × availableHeat
            ÷ totalIdealOutgoing
```

Remainder ordering:

```text
destination key
→ edge id
```

모든 granted transfer를 동시에 commit한다.

## 22.5 Ambient

Ambient cooling은 infinite ambient node와의 Thermal Edge로 같은 staging 규칙을 사용한다.

특별 in-place subtraction path를 만들지 않는다.

## 22.6 Heat Effect

Tick 시작 Temperature가 다음에 영향을 준다.

- Gate Delay
- Gate Drive
- Wire Delay
- Leakage
- Thermal Damage

현재 Tick에 새로 생긴 Heat는 다음 Tick Timing에 영향을 준다.

## 22.7 Electrical Damage

```text
electricalDamage = electricalEnergy / electricalTolerance
```

## 22.8 Thermal Damage

```text
thermalDamage = thermalDamageRate
              × max(0, T - safeTemperature)
```

## 22.9 Integrity

```text
integrityNext = integrity
              - electricalDamage
              - thermalDamage
```

부분 Integrity는 Timing / Drive를 직접 변경하지 않는다.

성능 저하는 Heat와 Brownout이 담당한다.

## 22.10 Simultaneous Destruction

Phase 10에서 `Integrity <= 0`이면 pending destruction이다.

현재 Tick 행동은 완료한다.

다음 Tick Phase 0에 제거한다.

## 22.11 Wire Destruction

Wire 제거 시 동시에 잃는다.

- Signal
- Power
- Sense
- Track
- Capacity Usage

Signal Arrival은 Path Certificate 규칙을 따른다.

Radiation Arrival은 유지한다.

## 22.12 Main Core / Relay

- Main Core 파괴: Phase 11 뒤 Run 종료
- Relay 파괴: 다음 Phase 0 Capacity 제거 / Site 유지

---

# 23. Quartz Runtime

## 23.1 Quartz Store

```rust
pub struct QuartzStore {
    pub ids: Vec<EntityId>,
    pub positions: Vec<FixedVec2>,
    pub output_drivers: Vec<DriverId>,
    pub power_attachments: Vec<TopologyNodeId>,
    pub integrity: Vec<Integrity>,
    pub heat_energy: Vec<HeatEnergy>,
}
```

Quartz Period는 Balance Profile의 versioned 값이다.

## 23.2 Canonical Phase

```text
phase = worldTick mod quartzPeriod

LOW  if phase < quartzPeriod / 2
HIGH otherwise
```

`quartzPeriod`는 짝수이고 2 이상이어야 한다. Profile Validation에서 강제한다.

## 23.3 Stability

Quartz internal phase와 period는 다음에 흔들리지 않는다.

- Fan-out
- Wire Length
- Brownout
- Heat

Quartz는 current World Tick에서 phase를 계산한다. Drift accumulator를 Canonical State로 유지하지 않는다.

## 23.4 Power와 Driver Strength

Quartz도 Output Driver를 구동하기 위한 Power Demand를 제출한다.

Power가 부족하면:

```text
internal phase continues
output effective strength → 0
```

Power가 복구되면 현재 World Tick phase의 Level을 일반 Driver Transition / Revision / Wire Delay 경로로 전달한다.

Quartz Level을 원격 Sink에 same-tick 즉시 쓰지 않는다.

## 23.5 Scheduling

- Phase 1: current World Tick phase sample
- Phase 3: desired quartz output intent
- Phase 4: drive demand
- Phase 6: 필요 시 Driver Transition 예약
- Phase 2 due Tick: Revision 증가와 Signal Arrival 생성

## 23.6 Quartz 없는 Clock

Feedback Oscillator는 Quartz 없이 만들 수 있다.

그 Period는 Gate / Wire Delay, Heat, Brownout의 영향을 받는다.

Quartz의 가치는 Clock 존재가 아니라 stable timing reference다.

## 23.7 Verification

최소 fixture:

- period / duty cycle
- Power loss 중 phase progression
- Power recovery current phase
- Heat / load 변화에도 period invariant
- normal Wire Delay를 거친 Sink waveform

---

# 24. Radiation Runtime

## 24.1 Source Unit

Radiation Source는 Wire polyline의 canonical straight segment다.

긴 polyline은 straight segment별 source를 가진다.

```rust
pub struct RadiationSourceSegment {
    pub wire_id: WireId,
    pub segment_index: u32,
    pub start: FixedVec2,
    pub end: FixedVec2,
    pub length: Fixed,
}
```

## 24.2 Switching Source

```text
HIGH → +strength
LOW  → -strength
X / no drive → 0
```

```text
Δdrive = signedDrive(t) - signedDrive(t-1)
```

`Δdrive = 0`이면 emission intent가 없다.

## 24.3 Emission Demand

```text
radiationDemand = f(
  abs(Δdrive),
  segmentLength,
  connectedLoad
)
```

Power Solve 뒤:

```text
0 <= emittedEnergy <= grantedSwitchingEnergy
```

방사되지 않은 Energy와 inefficiency는 Wire Heat다.

## 24.4 Radiation Cell

```rust
pub struct CellCoord {
    pub x: i64,
    pub y: i64,
}
```

Coordinate 변환은 `floor_div`를 사용한다.

## 24.5 Kernel Cache

Kernel은 다음 key로 reconstructible cache할 수 있다.

```text
segment quantized orientation
+ segment length band
+ radiation profile hash
```

Cache는 Canonical State에 포함하지 않는다.

## 24.6 Integer Kernel

```text
rawWeight = distanceWeight(distanceBand)
          × orientationWeight(orientationBand)
```

요구:

- finite radius
- distance monotonic decrease
- integer lookup table
- runtime float / atan2 없음
- deterministic cell enumeration

## 24.7 Source Budget Allocation

```text
W = escapeWeight + Σ rawWeight(cell)
```

```text
cellEnergy = emittedEnergy × rawWeight / W
```

Remainder ordering:

1. raw weight 큰 Cell
2. distance band 짧은 Cell
3. `(cellY, cellX)` lexicographic

```text
Σ cellEnergy <= emittedEnergy
```

## 24.8 Propagation

```text
arrivalTick = emissionTick
            + radiationDelay(distanceBand)
```

Delay는 최소 1 Tick이며 단조 비감소다.

## 24.9 Arrival Accumulation

Phase 8에서 due Arrival을 Cell별 합산한다.

```text
same Cell
+ same Tick
→ positive Energy sum
```

Source sign으로 cancellation하지 않는다.

## 24.10 Absorption

```text
targetWeight = absorption
             × crossSection
             × cellCoverage
```

```text
absorbed_i = cellEnergy
           × targetWeight_i
           ÷ (worldEscapeWeight + Σ targetWeight)
```

Remainder는 EntityId 오름차순이다.

## 24.11 Damage Output

Absorbed Energy는 Target Profile에 따라 Electrical / Thermal Exposure로 분할한다.

Radiation은 Damage Type이 아니다.

## 24.12 Debug / Analyzer

표시 가능 항목:

- Switching Event
- Emission Energy
- Kernel Footprint
- Arrival Tick
- Cell Energy
- Same-tick Accumulation
- Target Absorption


---

# 25. Command와 Laboratory Contract

## 25.1 Command Envelope

```rust
pub struct CommandEnvelope {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub command: Command,
}
```

같은 Tick에서 `ordinal`은 유일해야 한다.

## 25.2 Command Ordering

Phase 0에서 `ordinal` 오름차순으로 처리한다.

규칙:

1. 앞선 accepted Command의 결과를 뒤 Command가 본다.
2. 공간 / Entity 충돌이 생기면 뒤 Command를 deterministic reject한다.
3. Rejection은 Run Error가 아니다.
4. 중복 ordinal은 malformed input으로 거부한다.
5. Command Log가 Player 의도 순서의 정본이다.

## 25.3 Stage 0 Command

```rust
pub enum Command {
    PlaceGate(PlaceGateCommand),
    PlaceWire(PlaceWireCommand),
    PlaceJunction(PlaceJunctionCommand),
    PlaceFixedSubstrate(PlaceFixedSubstrateCommand),
    PlaceMobileSubstrate(PlaceMobileSubstrateCommand),
    RemoveEntity(RemoveEntityCommand),
    BindPort(BindPortCommand),
    SetExternalDriver(SetExternalDriverCommand),

    // Stage 1+
    PlaceConstructionSite(PlaceConstructionSiteCommand),
    PlaceModule(PlaceModuleCommand),
}
```

다음 semantic shortcut은 없다.

```text
CreateLatch
SetMobileDestination
RepairNetwork
ActivateDefenseSector
BuildRouter
```

## 25.4 Validation Pipeline

```text
1. target Tick
2. ordinal uniqueness
3. schema
4. Entity existence / lifecycle
5. Profile compatibility
6. Geometry quantization
7. Routing domain / footprint
8. Topology / overlap
9. ownership / interaction
10. atomic batch validation if required
11. apply or reject
```

## 25.5 Module Placement Atomicity

Module Placement 하나는 내부 Site 전체를 atomic command로 validate한다.

Validation이 성공하면 Primitive별 Construction Site를 생성한다.

일부만 생성한 뒤 나머지를 실패시키지 않는다.

실제 Primitive completion은 각 Site별로 독립이다.

## 25.6 Command Rejection

```rust
pub struct CommandRejection {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub reason: CommandRejectionReason,
}
```

대표 reason:

- DuplicateOrdinal
- WrongTick
- UnknownEntity
- InvalidGeometryQuantum
- InvalidRoutingPitch
- GeometryOverlap
- UnsupportedPlacement
- InvalidPortBinding
- ProfileMismatch
- AtomicModuleValidationFailed

## 25.7 Laboratory Live Edit

Laboratory와 World의 topology mutation Semantics는 같다.

Pause 중:

- Edit Command는 Host queue에 쌓인다.
- Canonical State는 즉시 바뀌지 않는다.
- 다음 Single Step / Resume의 Phase 0에 적용한다.
- Host는 Ghost Preview를 보여줄 수 있다.

Pending Event:

- 구조 변경과 무관한 Event 유지
- Path가 무효화된 Event만 일반 규칙으로 폐기
- 새 route는 TopologySyncArrival 생성

## 25.8 Laboratory Reset

Reset은 Command가 아니다.

```text
Current Simulation dispose
→ Scenario Initial Package load
→ new Simulation
→ Tick 0
→ no pending event
```

Reset 전후는 별도 Replay Session이다.

## 25.9 Laboratory Equivalence

같은 paused Canonical State와 같은 Edit Command Log를:

- Bevy Laboratory Single Step
- Headless Step

에서 실행하면 같은 State Hash가 나와야 한다.

---

# 26. Module, Artifact, Run Boundary

## 26.1 Module은 Blueprint다

Module은 Runtime Entity가 아니다.

```rust
pub struct ModuleBlueprint {
    pub format_version: ModuleFormatVersion,
    pub name: String,
    pub contract: ModuleContract,
    pub gates: Vec<GateBlueprint>,
    pub wires: Vec<WireBlueprint>,
    pub junctions: Vec<JunctionBlueprint>,
    pub substrates: Vec<SubstrateBlueprint>,
    pub io_bindings: Vec<ModulePortBinding>,
    pub provenance: ModuleProvenance,
}
```

## 26.2 Module Contract

```rust
pub struct ModuleContract {
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
}
```

Balance Profile Hash는 저장할 수 있으나 exact placement compatibility를 결정하는 필수 축은 아니다.

Analyzer baseline 비교에는 기록하는 것을 권장한다.

## 26.3 Absolute Geometry

Module은 absolute fixed-point geometry를 저장한다.

Host UI pixel, zoom, grid index를 저장하지 않는다.

## 26.4 Module Migration

Profile이 호환되지 않으면:

```text
silent resize 금지
```

Migration Tool은:

1. Source Module immutable load
2. explicit target profile 선택
3. geometry / routing 재검증
4. 새 Module ID / provenance 생성
5. 결과를 새 artifact로 저장

한다.

## 26.5 Module Placement

```text
Blueprint
→ command validation
→ Primitive Construction Site set
→ completed Primitive부터 activation
```

Module 전체를 하나의 HP / Runtime Component로 만들지 않는다.

## 26.6 Run Boundary

Main Core 파괴 시 Canonical World는 종료된다.

다음 Run으로 자동 이전되지 않는다.

- Power
- Capacity
- A/O/N Inventory
- Relay State
- Installed Circuit
- Wire Network
- Territory
- Pending Event

Module Library는 Run 바깥의 persistent artifact store다.

## 26.7 Artifact Format

Stage 0~MVP 초기 artifact는 versioned JSON을 허용한다.

대상:

- Profile
- Scenario
- Replay
- Module
- Experiment Manifest

JSON bytes 자체를 Canonical Hash 입력으로 사용하지 않는다.

---

# 27. Bevy Host Architecture

## 27.1 Plugin 구성

```text
AonHostPlugin
├─ SimulationHostPlugin
├─ InputCommandPlugin
├─ AsciiPresenterPlugin
├─ WaveformPlugin
├─ InspectorPlugin
├─ AnalyzerPlugin
└─ DebugOverlayPlugin
```

Stage 1 이후:

```text
CapacityOverlayPlugin
ExperimentControlPlugin
```

Stage 2 이후:

```text
RelayOverlayPlugin
```

## 27.2 Bevy Resource

```rust
#[derive(Resource)]
pub struct CanonicalSimulation(pub aon_sim::Simulation);

#[derive(Resource, Default)]
pub struct PendingCommands {
    pub commands: Vec<CommandEnvelope>,
}

#[derive(Resource, Default)]
pub struct LatestRenderSnapshot(pub RenderSnapshot);

#[derive(Resource, Default)]
pub struct LatestAnalyzerSnapshot(pub AnalyzerSnapshot);

#[derive(Resource, Default)]
pub struct SimulationHostState {
    pub pacer: SimPacer,
    pub single_step_requested: bool,
}
```

## 27.3 Schedule

```text
PreUpdate
→ input capture
→ UI intent
→ CommandEnvelope 생성
→ PendingCommands append

FixedUpdate
→ advance_canonical_simulation
→ 0회 이상 Simulation::step
→ StepReport collect

Update
→ snapshot refresh
→ interpolation
→ ASCII / UI / waveform / analyzer draw
```

## 27.4 Single Mutable Owner

`CanonicalSimulation`을 mutable access하는 Bevy System은 원칙적으로 하나다.

```text
advance_canonical_simulation
```

Renderer / Inspector / Analyzer는 Snapshot만 읽는다.

## 27.5 Host Pacing

```rust
pub struct SimPacer {
    pub mode: HostRunMode,
    pub tick_credit_numerator: u64,
    pub tick_credit_denominator: u64,
    pub accumulated_credit: u64,
}
```

Mode:

- Paused
- SingleStep
- Running 1x
- Fast
- Slow

Core는 wall-clock delta나 speed multiplier를 받지 않는다.

Host가 per-frame processing cap을 두더라도 Tick debt를 삭제하지 않는다.

## 27.6 Presentation Entity

Gate / Wire / Cell마다 무조건 Bevy Entity를 만들지 않는다.

Stage 0 권장:

```text
RenderSnapshot
→ CellBuffer
→ batched glyph draw
```

필요할 경우 Presentation mapping:

```rust
#[derive(Component)]
pub struct CanonicalId(pub EntityId);
```

이 mapping은 cache다.

## 27.7 ASCII Emergence Probe

ASCII는 최종 아트 결정이 아니라 첫 Simulation Debugger다.

```text
Interactive Game Probe
+ Oscilloscope
+ Topology Debugger
```

기본 glyph:

```text
빈 공간       ·
막힌 공간     #
Wire          ─ │ ┌ ┐ └ ┘ ┬ ┴ ├ ┤ ┼
AND           &
OR            |
NOT           !
Junction      ●
Fixed         ■
Mobile        ▲ ▶ ▼ ◀
Main Core     @
Relay         R
Logic X       glyph/background 이중 강조
```

Signal Level은 Wire geometry glyph를 바꾸기보다 brightness / foreground / background로 표현한다.

## 27.8 View

Stage 0에서는 continuous zoom을 요구하지 않는다.

최소:

```text
Network View
Circuit View
```

두 View는 같은 Canonical coordinate와 EntityId를 본다.

## 27.9 Waveform

필수:

- Pause
- Single Step
- Speed Control
- Last N Tick
- LOW / HIGH / X
- Probe add/remove
- Entity Inspector
- Driver Revision
- Arrival marker

Topology Sync debug에서는 Arrival kind를 표시할 수 있어야 한다.

## 27.10 Host Portability

`aon-sim`은 Post-MVP Web Alpha를 위해 수정되어서는 안 된다.

향후 Web Host를 추가하더라도:

```text
same Command / Snapshot / Replay contract
```

를 사용해야 한다.

본 TRD는 Web packaging 기술을 확정하지 않는다.

---

# 28. Snapshot, Analyzer, Telemetry

## 28.1 RenderSnapshot

```rust
pub struct RenderSnapshot {
    pub tick: Tick,
    pub contract: SimulationContract,
    pub gates: Vec<GateRenderRecord>,
    pub wires: Vec<WireRenderRecord>,
    pub junctions: Vec<JunctionRenderRecord>,
    pub mobiles: Vec<MobileRenderRecord>,
    pub main_core: MainCoreRenderRecord,
    pub relays: Vec<RelayRenderRecord>,
    pub enemies: Vec<EnemyRenderRecord>,
    pub construction_sites: Vec<ConstructionRenderRecord>,
}
```

Stage 0에서는 full snapshot copy를 허용한다.

병목이면 delta snapshot 또는 immutable shared buffer로 교체할 수 있다.

Snapshot 형식은 Semantics가 아니다.

## 28.2 StepReport

```rust
pub struct StepReport {
    pub completed_tick: Tick,
    pub signal_changes: Vec<SignalChangeRecord>,
    pub driver_changes: Vec<DriverChangeRecord>,
    pub command_rejections: Vec<CommandRejection>,
    pub topology_changed: bool,
    pub network_accounting: Option<NetworkAccountingReport>,
    pub relay_changes: Vec<RelayChangeRecord>,
    pub state_hash: Option<StateHash>,
    pub counters: StepCounters,
}
```

## 28.3 AnalyzerSnapshot

Analyzer는 의미를 부여하지 않는다.

```rust
pub struct AnalyzerSnapshot {
    pub physical: PhysicalMetrics,
    pub behavioral: BehavioralMetrics,
    pub network: NetworkMetrics,
    pub radiation: RadiationMetrics,
}
```

### Physical

- A/O/N Count
- Footprint
- Wire Length
- Network Capacity Usage
- Fan-out
- Delay
- Power
- Heat
- Construction Work

### Behavioral

- Stateful
- Periodic
- Stable / Unstable
- Reachable State Count
- Edge-sensitive

### Network

- Supported Capacity
- Used Capacity
- Excess
- Support Demand
- Relay State
- Power Margin

### Radiation

- Emission
- Kernel
- Arrival
- Absorption

Analyzer는 다음 label을 자동 생성하지 않는다.

```text
CPU
Memory
Router
Repair Brain
Fire Controller
```

## 28.4 Probe 비개입

Probe 등록 여부가 다음을 바꾸면 안 된다.

- Canonical State
- Event Ordering
- Memory Layout that affects result
- State Hash
- Performance-critical route selection

Stage 0에서는 모든 signal change를 반환할 수 있다.

규모가 커지면 Host-side filter를 사용한다.

## 28.5 Stage 1 Experiment Metric

최소 기록:

- Survival Tick / Time
- Total Wire Length / NCU
- Gate Count
- A/O/N Consumed
- Power Generation / Demand / Brownout
- Construction Work
- Heat
- Response Latency
- Enemy Kill / Core Damage
- Support Demand

## 28.6 Step Counter

Non-canonical telemetry:

```rust
pub struct StepCounters {
    pub driver_events_applied: u64,
    pub signal_arrivals_applied: u64,
    pub topology_sync_arrivals: u64,
    pub stale_revision_arrivals: u64,
    pub invalid_path_arrivals: u64,
    pub gates_evaluated: u64,
    pub routes_checked: u64,
    pub power_demands: u64,
    pub radiation_arrivals: u64,
    pub scratch_growths: u32,
}
```

Telemetry는 State Hash에 포함하지 않는다.

---

# 29. Replay와 State Hash

## 29.1 Replay Header

```rust
pub struct ReplayHeader {
    pub format_version: ReplayFormatVersion,
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
    pub balance_profile_hash: ProfileHash,
    pub world_generator_version: WorldGeneratorVersion,
    pub seed: Seed,
    pub initial_state_hash: StateHash,
    pub hash_algorithm_id: HashAlgorithmId,
}
```

## 29.2 Replay Body

```rust
pub struct Replay {
    pub header: ReplayHeader,
    pub commands: Vec<CommandEnvelope>,
    pub world_inputs: Vec<WorldInputEvent>,
    pub checkpoints: Vec<HashCheckpoint>,
}
```

Implicit random draw를 저장하지 않는다.

World randomness는:

- versioned PRNG + seed
- explicit WorldInputEvent

중 하나다.

## 29.3 Canonical Hash Encoder

규칙:

- fixed field order
- fixed-width little-endian integer
- EntityId ascending
- Driver / Sink ID ascending
- EventKey ascending
- Path Certificate ID ascending
- Profile Contract 포함
- cache 제외
- presentation 제외
- telemetry 제외
- wall-clock 제외

## 29.4 Hash 대상

포함:

- Canonical next Tick
- Canonical Entity State
- Driver Sample / Revision
- Sink Slot Sample / Revision
- Pending Event
- Pending Destruction / Relay Transition
- Construction / Relay Progress
- Thermal State
- Path Certificate
- Simulation Contract

제외:

- Compiled Route Cache
- Power Region Cache
- Spatial Index
- Analyzer Cache
- RenderSnapshot
- StepCounters

## 29.5 Checkpoint

Stage 0에서는 매 Tick checkpoint를 허용한다.

MVP에서는 configurable interval을 사용한다.

Divergence 시 최초 mismatch Tick을 찾는다.

## 29.6 Store Hash

Debug build / Headless diagnostic에서는 Store별 hash를 별도로 계산할 수 있다.

- Identity
- Geometry
- Gate
- Wire
- Driver / Sink
- Event
- Capacity / Relay
- Power / Thermal
- Mobility
- Radiation

World hash는 Store hash의 단순 concatenation hash로 구성할 수 있다.

## 29.7 Cross-host Oracle

동일 Replay를:

```text
aon-headless
Bevy aon-app
```

에서 실행해 모든 checkpoint가 같아야 한다.

## 29.8 Cross-platform

가능하면 Linux / Windows / macOS에서 golden replay hash를 비교한다.

Platform 차이로 hash가 달라지면 release blocker다.

---

# 30. Determinism, Parallelism, Performance

## 30.1 금지된 비결정성

Canonical Result는 다음에 의존하지 않는다.

- HashMap iteration order
- Thread scheduling
- Rayon work stealing
- Bevy system ordering
- CPU core count
- GPU reduction
- OS timer
- Rendering FPS
- pointer address
- locale
- wall-clock timestamp

## 30.2 Ordered Container 정책

Hash-based container를 lookup cache로 사용할 수 있다.

단:

- iteration 결과를 Canonical order로 사용하지 않는다.
- 결과 적용 전 stable key sort를 수행한다.
- State Hash에서 HashMap internal order를 encode하지 않는다.

Canonical order가 중요한 장기 collection은 `Vec + sort`, `BTreeMap`, sorted arena를 우선한다.

## 30.3 Performance 우선순위

```text
1. Semantics Correctness
2. Determinism
3. Debuggability
4. Algorithmic Scalability
5. Micro Optimization
```

## 30.4 Stage 0 Budget

- 20Hz Canonical Tick 유지
- 60fps Presentation 목표
- topology unchanged Tick allocation 0 목표
- Headless 100,000 Tick replay
- State Hash on/off 비용 분리 측정

구체적인 Gate / Wire scale은 benchmark fixture 관측 후 고정한다.

## 30.5 Stage 1 Budget

Parameter Sweep를 실용적으로 반복할 수 있어야 한다.

요구:

- Rendering 없이 headless run
- 동일 Scenario / Enemy Sequence 재사용
- Profile matrix batch execution
- machine-readable result artifact
- Replay / Profile hash 추적

## 30.6 Benchmark Fixture

Stage 0:

```text
bench_numeric_geometry
bench_gate_chain
bench_high_fanout
bench_feedback_ring
bench_many_independent_nets
bench_route_compile
bench_topology_sync
bench_stale_arrival
bench_mobile_junctions
bench_replay_hash
```

Stage 1:

```text
bench_capacity_accounting
bench_support_distribution
bench_power_regions
bench_sensing_grid
bench_contact_allocation
bench_thermal_grid
bench_construction_work
bench_profile_sweep
```

Stage 2:

```text
bench_relay_anchor_connectivity
bench_relay_state_transition
bench_relay_loss_crisis
```

MVP:

```text
bench_radiation_emitters
bench_radiation_arrivals
bench_spatial_queries
bench_payload_transfer
```

## 30.7 Optimization Escalation

Topology:

```text
Full Rebuild
→ Revision Cache
→ Dirty Component
→ Incremental Compile
```

Event:

```text
BinaryHeap
→ profile
→ Timing Wheel if needed
```

Spatial:

```text
Ordered Chunk Grid
→ profile
→ specialized broad phase
```

Render:

```text
Simple Batch Glyph
→ profile
→ Glyph Atlas / Instancing
```

## 30.8 미래 병렬화 후보

- Gate Intent Evaluation
- Radiation Kernel Allocation
- Thermal Edge Contribution
- Spatial Broad Phase
- Analyzer

각 Phase는 immutable input + independent buffer + deterministic reduction을 사용한다.

---

# 31. Test Architecture

## 31.1 테스트 계층

```text
Unit Test
→ numeric / collection / formula

Conformance Test
→ SSS 준수

Replay Golden Test
→ 장기 결정론

Property / Fuzz Test
→ 순서·topology·event 변형

Experiment Test
→ Stage 1 Crossover / Stage 2 Relay

Emergence Scenario
→ 상위 행동 가능성

Playtest
→ 실제 설계 욕구와 제품 가설
```

## 31.2 Stage 0 필수 Conformance

```text
C-01 Gate + Wire Delay
C-02 Inertial Filtering
C-03 Wire Transport
C-05 Feedback Ring
C-06 Symmetric Latch Startup
C-14 Mobile Junction
C-16 Replay Determinism
C-17 Numeric Geometry
C-18 Topology Synchronization
C-19 Stale Route Arrival
C-20 Same-tick Command Ordering
C-25 Laboratory Edit Equivalence
```

Stage 0 완료 전 권장:

```text
C-04 Fan-out Crossover
```

## 31.3 Stage 1 필수 Conformance

```text
C-07 Sensing
C-08 Brownout
C-09 Wire Break
C-10 Contact Energy Conservation
C-21 Capacity Accounting
C-22 Soft Overcapacity
```

## 31.4 Stage 2 필수 Conformance

```text
C-23 Relay Activation
C-24 Relay Loss
```

## 31.5 MVP 필수 Conformance

```text
C-11 Radiation Falloff
C-12 Radiation Geometry
C-13 Radiation Arrival Timing
C-15 Simultaneous Destruction
```

기존 Stage test도 계속 통과해야 한다.

## 31.6 Latch Emergence Scenario

Explicit Set / Reset을 가진 NOR-style Circuit Fixture를 둔다.

검증:

1. Startup Sequence가 명시됨
2. Set pulse 후 Q hold
3. Reset pulse 후 Q clear
4. `Latch` Runtime Class 없음
5. 일반 A/O/N / Delay / Feedback 결과

C-06의 symmetric startup과 구분한다.

## 31.7 Property Test

최소 property:

- due Tick Event insertion permutation independence
- Driver Slot order independence
- adjacency storage order와 path tie-break independence
- stale inertial event 미적용
- stale Revision arrival 미적용
- Topology Sync positive delay
- all causal delay >= 1
- invalid command no panic
- save/load final hash equality
- cache clear/rebuild equality
- geometry split length preservation
- capacity support monotonicity
- support share total conservation
- relay transition only Phase 0
- thermal exchange nonnegative / conservation
- radiation budget conservation

## 31.8 Fuzz Target

- Random quantized polyline
- Random Gate / Wire topology
- Random command ordinal sequence
- Random destroy / rebuild event
- Random driver transition / topology sync interleaving
- Random capacity / relay profile within invariant

Fuzz는 malformed input에서 panic이 없는지와 invariant violation을 탐지한다.

## 31.9 Cross-host Test

```text
same Initial Package
same Command Log
same World Input

Headless Hash == Bevy Hash
```

## 31.10 CI

최소:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p aon-sim --test conformance_stage0
cargo run -p aon-headless -- replay fixtures/replays/stage0-golden.json
```

Stage 1 이후:

```text
cargo test -p aon-sim --test conformance_stage1
cargo run -p aon-headless -- sweep experiments/stage-1/reference.json
```

Stage 2 이후:

```text
cargo test -p aon-sim --test conformance_stage2
```


---

# 32. Stage 0 Implementation Plan — Emergence Probe

Stage 0의 질문:

> **현재 Input만으로 해결할 수 없는 World 행동 때문에 State를 만들고 싶어지는가?**

Stage 0은 Capacity, Relay, Radiation, Payload, Enemy 4종을 구현하지 않는다.

## 32.1 S0-M0 — Bootstrap

구현:

- Cargo Workspace
- Rust toolchain pin
- `aon-sim`
- `aon-app`
- `aon-headless`
- CI
- empty Profile fixtures
- empty Scenario fixture

완료 조건:

- `aon-sim` dependency graph에 Bevy 없음
- Bevy Window가 empty RenderSnapshot 표시
- Headless / Bevy empty world hash 동일

## 32.2 S0-M1 — Contract / Numeric / Identity

구현:

- SimulationContract
- ProfileBundle load / hash / validation
- Numeric newtype
- Division helper
- `ceil_isqrt`
- FixedVec2 / Polyline Length
- EntityRegistry
- Stable EntityId
- ConnectionGeneration
- Canonical hash encoder skeleton

완료 조건:

- C-17 통과
- malformed profile deterministic rejection
- same profile semantic content → same hash
- EntityId 재사용 없음

## 32.3 S0-M2 — Command / Geometry / Structural Phase

구현:

- CommandEnvelope
- ordinal validation
- Place Gate / Wire / Junction / Substrate
- Remove Entity
- Bind Port
- Geometry quantization
- overlap / crossing rule
- Routing Domain
- Phase 0 skeleton
- Topology Revision

완료 조건:

- C-20 통과
- invalid geometry no panic
- same-tick command conflict deterministic

## 32.4 S0-M3 — Signal Topology / Event Runtime

구현:

- Driver / Sink Store
- GateStore
- WireStore
- Geometry Arena
- Signal Graph compile
- deterministic route
- Event Calendar
- DriverTransition
- SignalArrival
- Inertial generation token
- Sink resolve
- State Hash event encoding

완료 조건:

- C-01 통과
- C-02 통과
- C-03 통과

## 32.5 S0-M4 — Topology Sync / Path Certificate

구현:

- Route Diff
- Driver Revision
- TopologySyncArrival
- Sink Slot revision
- PathCertificateArena
- Connection Generation validation
- stale arrival discard

완료 조건:

- C-18 통과
- C-19 통과
- destroy / rebuild old Event invalidation

## 32.6 S0-M5 — Feedback / Replay

구현:

- feedback route
- startup LOW
- ring oscillator fixture
- symmetric latch fixture
- explicit Set/Reset emergence fixture
- Replay Header / Command Log / Checkpoint
- Cross-host golden replay

완료 조건:

- C-05 통과
- C-06 통과
- C-16 통과
- Headless 100,000 Tick fixture

## 32.7 S0-M6 — Bevy ASCII Probe

구현:

- CellBuffer Presenter
- Primitive placement / delete / bind
- Pause / Single Step / Speed
- Signal Probe
- Waveform
- Driver Revision marker
- Topology Sync marker
- Inspector
- Network / Circuit LOD
- Laboratory Reset

완료 조건:

- FPS 변경 시 Replay Hash 동일
- UI Preview가 Canonical State를 직접 변경하지 않음
- C-25 통과

## 32.8 S0-M7 — Mobility

구현:

- Track Graph
- TrackPosition
- STOP / LEFT / RIGHT
- Junction Turn Ordering
- Dead-end reverse
- Stage 0에서는 power ratio 1.0
- Mobile rendering / inspector

완료 조건:

- C-14 통과
- A→Junction→B→State→STOP/RETURN Scenario 동작
- FSM / RoutePlanner Runtime Class 없음

## 32.9 Stage 0 Technical Gate

Architecture:

- Pure Rust Canonical Core
- Bevy Host 분리
- Stable EntityId
- Explicit 12 Phase
- Event-based Signal
- Driver Revision / Sync / Path Certificate
- Replay Oracle

Conformance:

```text
C-01 C-02 C-03 C-05 C-06 C-14
C-16 C-17 C-18 C-19 C-20 C-25
```

## 32.10 Stage 0 Product Gate

다음을 직접 플레이해 판정한다.

```text
현재 입력만 읽는 설계
vs
과거 상태를 유지하는 설계
```

PASS:

- 별도 Runtime Class 없이 State 행동 생성
- State가 단순 퍼즐 장식이 아니라 World 행동에 필요
- 회로를 디버깅하고 개선하려는 욕구가 발생

FAIL:

- Latch는 만들 수 있으나 만들 이유가 없음
- State 없이도 Scenario가 동일하게 풀림
- 행동 결과의 인과를 읽을 수 없음

FAIL이면 Stage 1로 진행하지 않는다.

---

# 33. Stage 1 Implementation Plan — Capacity Economy Probe

Stage 1의 질문:

> **Computation이 실제로 Infrastructure를 대체하는가?**

구현 범위:

```text
A/O/N
Wire
Main Core
Global Capacity
Sensing
Power / Brownout
Heat / Damage
Contact Attack
Construction Work
Enemy 1종
```

불필요:

```text
Relay
Radiation
Payload Repair Loop
Quartz
Enemy 4종
```

## 33.1 S1-M0 — Physical Scale Experiment Baseline

구현:

- Stage 0 Physical Scale Profile load
- Profile matrix generator
- Gate Footprint variation
- Circuit Routing Pitch variation
- World Routing Pitch variation
- Long-wire distance variation
- Module absolute geometry validation

완료 조건:

- 각 Run이 unique Physical Scale Profile Hash 보유
- Module silent scaling 없음
- 동일 Profile replay 가능

## 33.2 S1-M1 — Main Core / Capacity Accounting

구현:

- MainCoreState
- Core Capacity
- Active Wire Length Accounting
- Internal Wire 포함
- Multi-role one-time accounting
- Network Analyzer

완료 조건:

- C-21 통과
- Segment split / polyline vertex length preservation

## 33.3 S1-M2 — Sensing / Power / Brownout

구현:

- Spatial Index
- Wire Capsule Sensing
- Sense Driver Delay
- Power Graph / Region
- Canonical Source Route
- Region `ρ` Solver
- Gate / Sense / Movement grant seam
- Leakage / Transmission Heat

완료 조건:

- C-07 통과
- C-08 통과

## 33.4 S1-M3 — Capacity Support Load

구현:

- Excess calculation
- Soft Support Curve
- Wire Length proportional distribution
- EntityId remainder
- intrinsic Power Demand
- Support Heat
- Relay 없는 Supported Capacity = Main Core only

완료 조건:

- C-22 통과
- Demand monotonic property
- no build rejection
- no direct capacity damage / delay modifier

## 33.5 S1-M4 — Construction / Contact / Damage

구현:

- Construction Site
- Wire length based Work
- Gate / Junction / Substrate Work
- active Phase 0 commit
- Enemy 1종 deterministic movement
- Live Wire Demand
- Contact allocation
- Electrical / Thermal Damage
- Main Core run end

완료 조건:

- C-09 통과
- C-10 통과
- same Tick destruction behavior 유지

## 33.6 S1-M5 — Reference Architecture Fixture

두 Design을 artifact로 고정한다.

### Design A — Brute Force

```text
Dense Sensor Grid
Dedicated Signal Lines
Large Defense Coverage
Minimal Logic
```

### Design B — Computed

```text
Sparse / Local Sensor
State
Local Processing
Shared Long Lines
Selective Defense
```

두 Design은 동일한:

- Power
- Core Capacity
- Territory
- Enemy Sequence
- Seed
- Semantics / Numeric Contract

를 사용한다.

## 33.7 S1-M6 — Parameter Sweep

Sweep axis:

- Gate Footprint
- Circuit Routing Pitch
- World Routing Pitch
- Long-wire Distance
- Main Core Capacity
- Support Curve Coefficient

최소 metric:

- Survival
- Wire Length / NCU
- Gate Count
- A/O/N Consumed
- Power
- Construction Work
- Heat
- Response Latency

## 33.8 Stage 1 Technical Gate

Conformance:

```text
C-07 C-08 C-09 C-10 C-21 C-22
```

Regression:

```text
all Stage 0 tests
```

## 33.9 Stage 1 Product Gate

다음 세 영역이 모두 실제 Layout에서 관찰되어야 한다.

### Early-scale

Brute Force가 충분히 유효하다.

### Crossover

Brute와 Computed가 실질적으로 경쟁한다.

### Large-scale

Computed가 더 많은 Gate를 사용하더라도 더 적은 NCU로 높은 확장 효율을 얻는다.

FAIL:

- 모든 Scale에서 Wire 복제가 지배
- 모든 Scale에서 Computed가 강제 정답
- Capacity coefficient만 과도하게 올려 인위적 Crossover 생성
- Capacity 최소화가 Power / Delay / Heat 등 모든 축을 압도

FAIL이면 Relay를 구현하기 전에 H2와 Physical Scale을 재검토한다.

---

# 34. Stage 2 Implementation Plan — Relay Expansion Probe

Stage 2의 질문:

> **Relay가 Network Expansion과 전략적 Territory 선택을 만드는가?**

## 34.1 S2-M0 — Relay World Fixture

구현:

- Main Core
- 2~3 Relay Site
- intact Offline Relay Structure
- deterministic Terrain / Enemy Pressure
- Anchor attachment

## 34.2 S2-M1 — Relay Store / Anchor Connectivity

구현:

- RelaySiteStore
- Structure ID
- Body Connectivity compile
- Core anchor reachability
- Analyzer state

## 34.3 S2-M2 — Activation / Upkeep / Restart

구현:

- Activation Intent
- Activation Demand / Work
- hasEverBeenOnline
- next Phase 0 Online transition
- Online Upkeep
- unhealthy counter
- Offline Grace
- Restart Work

완료 조건:

- C-23 통과
- one-Tick toggle 없음

## 34.4 S2-M3 — Destruction / Reconstruction Site

구현:

- Relay Damage
- next Phase 0 contribution removal
- Site persistence
- Reconstruction Site
- new Structure EntityId
- Offline restart state

완료 조건:

- C-24 통과
- Existing Wire 유지
- Capacity Crisis 관찰 가능

## 34.5 Stage 2 Product Gate

검증:

- Relay 확보가 확장 동기를 만드는가.
- Relay Loss가 실제 Network Crisis를 만드는가.
- 새 Relay 확보와 기존 Network 압축이 경쟁하는가.
- Relay가 단순 Map Unlock Token 이상인가.

FAIL이면 Relay Capacity Amount보다 먼저 구조적 역할을 재검토한다.

---

# 35. MVP Expansion Plan — Emergent Defense

Stage 0·1·2 PASS 이후 시작한다.

## 35.1 MVP-M0 — Payload / Transfer

- A/O/N Cargo Unit
- LOAD / UNLOAD
- Payload Mass / Footprint
- Multi-mobile allocation

## 35.2 MVP-M1 — Full Reconstruction Loop

- Gate / Wire Reconstruction Site
- Fault signal observation
- Cargo acquisition
- Shared Track routing
- BUILD
- partial Module construction

## 35.3 MVP-M2 — Quartz

- Global phase output
- stable period
- power-gated strength
- non-quartz oscillator comparison

## 35.4 MVP-M3 — Radiation

- Switching Source
- Integer Kernel
- Source Budget
- Propagation Delay
- Arrival Accumulation
- Absorption
- Debug Overlay

완료 조건:

```text
C-11 C-12 C-13
```

## 35.5 MVP-M4 — Enemy Pressure Set

- Assault
- Ranged
- Drop / Artillery
- Suicide

Enemy는 특정 Circuit key를 요구하지 않는다.

## 35.6 MVP-M5 — Module Library

- Blueprint save/load
- Contract compatibility
- explicit migration
- Variant Analyzer
- low-wire / low-power / compact 비교

## 35.7 MVP-M6 — Laboratory Expansion

- Sensing Overlay
- Capacity Overlay
- Relay Overlay
- Power / Heat Overlay
- Radiation Overlay
- Mobile Test Track
- Test Enemy

## 35.8 MVP 핵심 Scenario

```text
Drop Enemy 착탄
→ Backbone 파괴
→ Local Circuit Fault 추론
→ Task State 유지
→ Mobile 출발
→ Shared Network Routing
→ A/O/N Cargo
→ BUILD
→ Network 복구
```

금지 Runtime Class:

```text
RepairBot
FSM
Memory
RoutePlanner
```

## 35.9 MVP Gate

- Emergence
- State value
- Computation substitutes Infrastructure
- Brute remains valid
- Crossover exists
- No blanket-wire dominance
- Capacity does not dominate everything
- Relay expansion pressure
- Module optimization desire
- Abstraction value
- Multiple solutions

---

# 36. Experiment Harness

Stage 1과 Stage 2의 Product Gate는 수동 느낌만으로 판정하지 않는다.

## 36.1 Experiment Manifest

```rust
pub struct ExperimentManifest {
    pub experiment_id: String,
    pub stage: ExperimentStage,
    pub scenario_path: String,
    pub design_variants: Vec<DesignArtifactRef>,
    pub profile_matrix: ProfileMatrix,
    pub seeds: Vec<Seed>,
    pub max_ticks: Tick,
    pub metric_set: MetricSet,
}
```

## 36.2 Design Artifact

Brute / Computed Design은 동일 형식의 Module / World Build Command artifact로 저장한다.

손으로 코드에 박은 별도 AI나 rule을 사용하지 않는다.

## 36.3 Profile Matrix

```rust
pub struct ProfileMatrix {
    pub numeric_profiles: Vec<ProfileRef>,
    pub physical_scale_profiles: Vec<ProfileRef>,
    pub balance_profiles: Vec<ProfileRef>,
}
```

Numeric Profile은 v1에서 보통 고정한다.

Physical Scale와 Balance 계수를 sweep한다.

## 36.4 Reproducibility

각 Run은 다음을 기록한다.

- Experiment ID
- Scenario Hash
- Design Artifact Hash
- Simulation Contract
- Seed
- Command Log Hash
- Final State Hash
- Metric Artifact Hash
- Build Commit ID

## 36.5 Output

Machine-readable canonical result:

```text
JSON Lines 또는 versioned binary record
```

사람용 export:

```text
CSV summary
Markdown report
```

사람용 formatting은 canonical result가 아니다.

## 36.6 Crossover Report

최소 축:

```text
X = Problem / Distance Scale
Y1 = Survival Efficiency
Y2 = NCU Usage
Y3 = Power / Heat / Work
```

결과는 Early / Crossover / Large 영역을 표시한다.

## 36.7 Anti-overfitting Rule

다음 방식으로 PASS를 조작하지 않는다.

- Computed Design에만 유리한 숨은 multiplier
- Brute Design에만 불리한 enemy key
- 단일 Profile 결과만 선택
- 실패 Profile 삭제
- Module geometry silent rescale
- Capacity curve를 사실상 hard limit로 만들기

Parameter Sweep 전체 결과를 보존한다.

---

# 37. Error Handling과 Diagnostics

## 37.1 Simulation Error

```rust
pub enum SimulationError {
    NumericOverflow,
    InvalidCanonicalState,
    ProfileHashMismatch,
    UnsupportedSemanticsVersion,
    UnsupportedProfileVersion,
    InvalidPhysicalScaleProfile,
    TopologyCompileFailure,
    EventQueueInvariantViolation,
    DriverRevisionInvariantViolation,
    PathCertificateInvariantViolation,
    ReplayVersionMismatch,
    ReplayContractMismatch,
    HashAlgorithmMismatch,
}
```

## 37.2 Player Error

Player Command 오류는 `SimulationError`가 아니라 `CommandRejection`이다.

Run을 중단하지 않는다.

## 37.3 Panic 정책

- Player Input으로 panic 금지
- malformed Replay로 panic 금지
- unsupported Profile로 panic 금지
- debug build에서 internal `debug_assert!`
- release에서 deterministic error return

## 37.4 Divergence Dump

Replay mismatch 시:

- Tick
- Phase
- World Hash
- Store Hash
- First differing EntityId
- Driver / Sink Revision
- Pending Event Count
- First differing EventKey
- Topology Revision
- Capacity Accounting
- Relay Mode

를 출력할 수 있어야 한다.

## 37.5 Topology Diagnostics

- Added / Removed Route
- TopologySyncArrival count
- invalid Path Certificate element
- connection generation mismatch
- anchor connectivity path

## 37.6 Experiment Diagnostics

- Profile validation failure
- Scenario incompatibility
- Design artifact incompatibility
- Run timeout
- Core destroyed Tick
- Metric collection completeness

---

# 38. 주요 리스크와 가드레일

## R1. Bevy ECS가 Canonical World가 되는 위험

징후:

- Gate가 Bevy Component로만 존재
- Bevy Entity가 Replay에 저장
- Core Phase가 여러 Bevy System에 분산

가드레일:

- `aon-sim` Bevy dependency 금지
- Single mutable owner
- Cross-host Replay

## R2. Signal을 synchronous double buffer로 축소할 위험

징후:

```text
signal_state / next_signal_state만 존재
```

손실:

- Inertial cancellation
- Transport Pulse
- Per-route Delay
- Topology Sync
- Driver Revision
- Glitch / Hazard

가드레일:

- Event Runtime 필수
- C-02 / C-03 / C-18 / C-19 필수

## R3. Profile이 숨은 Semantics가 되는 위험

가드레일:

- formula shape는 SSS
- coefficient만 Balance Profile
- canonical hash
- unsupported field rejection
- experiment artifact에 hash 기록

## R4. Physical Scale로 H2를 조작할 위험

가드레일:

- Parameter Sweep 전체 보존
- Module absolute geometry
- silent scaling 금지
- realistic Layout에서 Crossover 없으면 H2 실패

## R5. Capacity Cache가 Truth가 되는 위험

가드레일:

- Active Wire에서 재계산 가능
- State Hash에 cache 제외
- cache clear/recompute test
- one-body one-length invariant

## R6. Capacity가 모든 최적화를 지배할 위험

가드레일:

- Gate Count / A/O/N / Power / Delay / Heat / Work 함께 측정
- Overcapacity direct penalty 금지
- Early Brute validity 확인

## R7. Relay가 Map Unlock Token이 될 위험

가드레일:

- Relay output은 Capacity뿐
- 확보 비용은 actual Wire / Power / Defense
- Loss는 Existing Network Overcapacity
- Expansion vs Compression playtest

## R8. Relay Power Feedback가 Tick Oscillation을 만들 위험

가드레일:

- Mode Phase 0 only
- Activation / Hold 분리
- Offline Grace
- next-Tick transition

## R9. TopologySync Event 폭증

큰 topology edit에서 `(Driver, Sink)` added route가 폭발할 수 있다.

가드레일:

- 먼저 correctness 유지
- StepCounters 기록
- route component별 batching 검토
- Event 생략 최적화는 waveform equivalence proof 필요

## R10. Path Certificate Memory 증가

가드레일:

- reference count telemetry
- consumed tombstone ratio
- deterministic arena generation
- pending Event가 있는 ID 재사용 금지

## R11. Custom SoA 타입 안전성 저하

가드레일:

- typed index
- Store mutation API 중앙화
- length invariant test
- debug validation
- raw usize 외부 노출 금지

## R12. Stage 0 Probe가 최종 UI 작업으로 팽창

가드레일:

- continuous zoom 보류
- Stage별 overlay만 추가
- Presenter는 Core와 분리
- Product Gate 전 polish 제한

## R13. Post-MVP Web Alpha가 Core를 오염할 위험

가드레일:

- Host-neutral Core API
- Replay Oracle
- Web packaging 결정 유예
- Canonical float / wall-clock 도입 금지

---

# 39. Architecture Decision Records

TRD 승인 후 다음 ADR을 index로 생성한다.

## ADR-001 — Pure Rust Canonical Simulation

Canonical World와 상태 전이는 `aon-sim`이 소유한다.

## ADR-002 — Bevy as Interactive Host Only

Bevy ECS는 Presentation과 Host Integration에만 사용한다.

## ADR-003 — Custom SoA, No Canonical ECS

Canonical Storage는 explicit SoA Store를 사용한다.

## ADR-004 — Explicit 12-Phase Tick Engine

모든 Canonical Phase는 `Simulation::step()` 안에서 실행한다.

## ADR-005 — Versioned Simulation Contract

Run은 Semantics / Numeric / Physical Scale / Balance Contract를 가진다.

## ADR-006 — Canonical Fixed-point Geometry

`FIXED_ONE`, division, `ceil_isqrt`, floor coordinate를 정본 구현한다.

## ADR-007 — Event-based Signal Runtime

Gate Inertial과 Wire Transport를 Event로 구현한다.

## ADR-008 — Driver Revision and Topology Synchronization

새 route는 current sample을 Delay 뒤 동기화하고 stale arrival을 Revision으로 막는다.

## ADR-009 — In-flight Path Certificate

Signal Arrival은 ordered Wire / Junction ID와 connection generation을 보존한다.

## ADR-010 — Reconstructible Compiled Topology

Signal / Power / Track / Body graph는 revision cache다.

## ADR-011 — Stable Canonical Identity

EntityId는 Run 내 재사용하지 않고 Bevy Entity와 분리한다.

## ADR-012 — Global Capacity as Derived Wire-length Accounting

Capacity Usage는 Active Wire Length에서 파생한다.

## ADR-013 — Soft Overcapacity through Power and Heat

Capacity 초과는 Support Demand와 Heat로만 작용한다.

## ADR-014 — Relay as Capacity-only World Resource

Relay는 Online일 때 Capacity만 제공한다.

## ADR-015 — Replay as Determinism Oracle

Headless와 Bevy Host가 같은 Tick Hash를 만든다.

## ADR-016 — Single-thread Reference First

정본 hash가 확립되기 전 Canonical Core를 병렬화하지 않는다.

## ADR-017 — Stage-gated Implementation

Stage 0·1·2 Product Gate 통과 전 다음 Stage 전체 구현을 시작하지 않는다.

## ADR-018 — Parameter Sweep as Stage 1 Evidence

Crossover는 단일 Balance 값이 아니라 versioned Profile Sweep으로 검증한다.

---

# 40. Definition of Done

## 40.1 Stage 0 DoD

### Architecture

- `aon-sim`에 Bevy 없음
- Stable EntityId
- Simulation Contract 검증
- Explicit 12 Phase
- Event Runtime
- Driver Revision / Topology Sync / Path Certificate
- Headless / Bevy hash equality

### Semantics

- LOW / HIGH / X
- AND / OR / NOT
- Inertial Gate
- Transport Wire
- Positive-delay Feedback
- Startup LOW
- Command ordinal
- Laboratory live edit
- STOP / LEFT / RIGHT

### Verification

```text
C-01 C-02 C-03 C-05 C-06 C-14
C-16 C-17 C-18 C-19 C-20 C-25
```

### Probe UX

- Place / Remove / Bind
- Pause / Step / Speed
- Waveform
- Revision / Arrival Debug
- Inspector
- Mobile Scenario

### Product

- State를 만들 이유가 실제로 느껴짐

## 40.2 Stage 1 DoD

### Systems

- Main Core
- Capacity Accounting
- Sensing
- Power / Brownout
- Support Demand / Heat
- Construction
- Contact Attack
- Enemy 1종

### Verification

```text
C-07 C-08 C-09 C-10 C-21 C-22
```

### Experiment

- Brute / Computed artifact
- Profile Matrix
- Reproducible Sweep
- Early / Crossover / Large report

### Product

- Computation / Infrastructure crossover 존재
- Brute early validity 유지
- Capacity가 유일 최적화 축이 아님

## 40.3 Stage 2 DoD

### Systems

- Relay Site
- Anchor Connectivity
- Activation / Upkeep / Restart
- Relay Loss
- Reconstruction Site

### Verification

```text
C-23 C-24
```

### Product

- Relay 확보 동기
- Loss Crisis
- Expansion vs Compression 경쟁

## 40.4 MVP DoD

### Systems

- Payload / Transfer
- Full Reconstruction
- Quartz
- Radiation
- 4 Enemy Types
- Module Library
- Expanded Laboratory

### Verification

```text
C-11 C-12 C-13 C-15
+ all previous Stage tests
```

### Product

PRD V1~V11을 검증한다.

---

# 41. Source Baseline와 변경 관리

이 TRD는 다음 정본을 기준으로 한다.

- A/O/N Product Requirements Document v1.0 GO
- A/O/N Simulation Semantics Specification v1.0 Draft
- Rust 2024 Edition
- Bevy 0.19.x implementation baseline

## 41.1 Compatibility Review

PRD 또는 SSS version이 바뀌면 다음을 수행한다.

1. Product Stage / Gate 변경 확인
2. Observable Semantics diff
3. SimulationContract version 영향
4. Canonical State schema 영향
5. Event / Phase 영향
6. Conformance fixture 영향
7. Replay / Module compatibility 영향
8. TRD section update

## 41.2 Semantics 변경

Observable Law가 바뀌면:

- Semantics Version 증가
- Conformance Test 갱신
- Golden Replay 분리
- Module compatibility 판단
- Experiment result lineage 분리

## 41.3 Implementation 변경

같은 Tick Hash를 유지하는:

- Cache
- Data Layout
- Event Queue
- Spatial Index
- Parallelization
- Renderer

변경은 TRD / ADR revision만으로 처리할 수 있다.

---

# 42. 최종 구현 불변식

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

Host도 이 등식을 우회하지 않는다.

```text
Headless
= Bevy Native
= Future Host
```

Capacity도 이 등식을 우회하지 않는다.

```text
Wire를 더 깐다
→ actual Length 증가
→ Capacity Usage 증가
→ 필요하면 Support Demand 증가
→ Power / Heat Margin 감소
```

계산으로 Wire를 줄이면 그 반대가 일어난다.

그리고:

```text
Bevy를 제거해도 A/O/N World는 남아야 한다.
```

> **문명은 무너진다. 설계는 남는다. Host는 바뀌어도 Semantics는 남는다.**
