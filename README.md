# A/O/N

A/O/N은 AND, OR, NOT과 하나의 범용 Wire로 감지, 이동, 물류, 방어와 계산을 구성하는 결정론적 시뮬레이션 게임이다.

**`S0-M0 — Bootstrap`부터 `S0-M3 — Signal Topology / Event Runtime`까지 완료됐고,
`S0-M4 — Topology Sync / Path Certificate`가 다음 구현 경계다.** 정적 topology의
Gate/Wire signal runtime은 구현됐지만, in-flight topology edit의 stale-arrival 처리와
path certificate는 아직 구현되지 않았다. Stage 0과 전체 엔진은 아직 완료되지 않았으며
현황은 `docs/AON_Game_Engine_Implementation_Tracker_v1.0.md`에서 추적한다.

## Source baseline

- Product: `docs/prd-v.1.0.md` — v1.0 GO Candidate
- Semantics: `docs/AON_Simulation_Semantics_Spec_v1.0.md` — v1.0 Draft
- Architecture: `docs/AON_TRD_v1.0.md` — v1.0 Draft
- Engine implementation tracker: `docs/AON_Game_Engine_Implementation_Tracker_v1.0.md`
- Bootstrap milestone archive: `docs/AON_Engine_Bootstrap_Milestones_v1.0.md` — v1.0 Draft

PRD §57의 SSS v0.2 언급은 현재 파일보다 오래된 기준선이다. 구현은 SSS v1.0과 TRD v1.0을 기준으로 한다.

## Pinned environment

```text
Rust: 1.97.1
Cargo: 1.97.1
Edition: 2024
Bevy: 0.19.0
Initial native target: Linux
CI: Ubuntu 24.04
```

`rust-toolchain.toml`이 `rustfmt`와 `clippy`를 포함한 exact Rust toolchain을 선택하고 `Cargo.lock`이 dependency graph를 고정한다.

Ubuntu/WSL에서 Native Host를 빌드하고 실행하려면 다음 시스템 패키지가 필요하다.

```bash
sudo apt-get install build-essential pkg-config libxkbcommon-x11-0
```

## Workspace

```text
crates/aon-sim     Pure Rust canonical simulation core
crates/aon-fuzz-harness  Bounded decoder/geometry fuzz regression harness
apps/aon-headless  Scenario runner and determinism oracle
apps/aon-app       Bevy interactive host shell
```

Dependency 방향은 Host에서 Core로만 흐른다. `aon-sim`은 Bevy, winit, wgpu, wall-clock 또는 frame delta에 의존하지 않는다.

## Run

Headless empty scenario:

```bash
cargo run -p aon-headless -- \
  scenario fixtures/scenarios/empty.json --ticks 1
```

Native Bevy host:

```bash
cargo run -p aon-app
```

Native host는 빈 World를 window title로 표시한다. Window가 실행되는 동안 title에 현재 canonical tick과 state hash prefix가 나타난다.

## Quality gates

```bash
cargo metadata --format-version 1 --no-deps --locked --offline
cargo check --workspace --all-targets --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo test -p aon-sim --test conformance_stage0 --locked --offline
cargo test -p aon-sim --test signal_conformance --locked --offline
cargo test -p aon-fuzz-harness --locked --offline
```

CI의 cross-host test는 0, 1, 100 tick에서 Headless loop와 실제 Bevy `FixedUpdate` schedule의 전체 hash trace를 비교한다. Native window 실행은 display server가 필요한 수동 smoke gate다.

## Current API boundary

현재 Core API는 versioned `SimulationContract`, 실제 Numeric/Physical/Balance Profile,
fixed-point numeric/geometry, stable identity registry, canonical Stage 0 structural command,
Driver/Sink signal state, deterministic event calendar, `Simulation::new`, transactional
`Simulation::step`, render snapshot, canonical state hash까지 제공한다.

- Scenario, Numeric, Physical schema `1`과 Balance schema `2`, semantics
  `aon-semantics-v1`, hash algorithm `blake3-v1`을 현재 지원한다.
- Profile hash는 versioned canonical encoding을, state hash는 signal/event sections를 포함한
  `AON\0STATE\0V2\0` encoder를 사용한다.
- artifact의 Initial World는 아직 Empty만 지원하지만, command로 Fixed Substrate, Gate, Wire,
  Junction을 생성·바인드·제거할 수 있다.
- Phase 0은 ordinal ordering, deterministic rejection, geometry validation, generation/revision,
  fatal-overflow rollback을 구현한다.
- Phase 2/3/6은 DriverTransition, SignalArrival, simultaneous Sink resolution, Gate truth table,
  inertial cancellation, transport delay를 구현한다.
- `S0-M4`가 topology-sync event, Driver/Sink revision, path certificate, in-flight topology
  edit의 stale-arrival rejection을 이어서 구현한다.
