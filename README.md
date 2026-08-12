# A/O/N

A/O/N은 AND, OR, NOT과 하나의 범용 Wire로 감지, 이동, 물류, 방어와 계산을 구성하는 결정론적 시뮬레이션 게임이다.

**Stage 0 Emergence Probe 전체가 정식 완료됐다.** S0-M6 Bevy ASCII Probe와 S0-M7
Mobility는 커밋 `bf651c9e63aed41e6db7e0f7b2767345834ff94e`의 독립 Windows-native
fresh clean-checkout offline 게이트와 실제 GUI 스모크를 통과했고, Stage 0 product gate는
2026-08-12 사용자 직접 A/B 플레이에서 PASS했다. 전체 엔진은 아직 완료되지 않았으며, 현황은
`docs/AON_Game_Engine_Implementation_Tracker_v1.0.md`에서 추적한다.

## Source baseline

- Product: `docs/prd-v.1.0.md` — v1.0 GO Candidate
- Semantics: `docs/AON_Simulation_Semantics_Spec_v1.0.md` — v1.0 Draft
- Architecture: `docs/AON_TRD_v1.0.md` — v1.0 Draft
- Engine implementation tracker: `docs/AON_Game_Engine_Implementation_Tracker_v1.0.md`
- Bootstrap milestone archive: `docs/AON_Engine_Bootstrap_Milestones_v1.0.md` — v1.0 Draft
- S0-M5 implementation authority: `docs/AON_S0_M5_Canonical_Decisions_v1.0.md`
- S0-M6 implementation authority: `docs/AON_S0_M6_Canonical_Decisions_v1.0.md`
- S0-M7 implementation authority: `docs/AON_S0_M7_Canonical_Decisions_v1.0.md`
- Stage 0 product playtest: `docs/AON_Stage0_Product_Gate_Playtest_v1.0.md`

PRD §57의 SSS v0.2 언급은 현재 파일보다 오래된 기준선이다. 구현은 SSS v1.0과 TRD v1.0을 기준으로 한다.

## Pinned environment

```text
Rust: 1.97.1
Cargo: 1.97.1
Edition: 2024
Bevy: 0.19.0
Primary local native target: Windows / PowerShell
Milestone verification: Windows native; WSL is not used
CI: Ubuntu 24.04 + windows-latest
```

`rust-toolchain.toml`이 `rustfmt`와 `clippy`를 포함한 exact Rust toolchain을 선택하고 `Cargo.lock`이 dependency graph를 고정한다.

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

Headless bounded feedback Replay:

```bash
cargo run -p aon-headless -- \
  replay fixtures/replays/feedback-ring-v1.json
```

Retained 100,000-Tick Stage 0 Replay:

```bash
cargo run -p aon-headless -- \
  replay fixtures/replays/stage0-100k-v1.json
```

Retained Mobility/feedback Replay:

```powershell
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/mobility-retained-stop-v1.json
```

Native Bevy host:

```bash
cargo run -p aon-app
```

Windows-native Stage 0 direct-play A/B product probe:

```powershell
cargo run -p aon-app --locked --offline -- stage0-product-probe
```

이 프리셋은 동일한 SET 입력을 받는 두 Mobility 설계를 Tick 24에서 SET/Q/Qbar/STOP probe와
함께 일시정지한다. `F5`는 current-input-only control, `F6`은 retained-state design을 같은
준비 지점에서 새로 연다. 직접 플레이 절차와 판정 양식은
`docs/AON_Stage0_Product_Gate_Playtest_v1.0.md`에 있다.

Native host는 빈 World를 window title로 표시한다. Window가 실행되는 동안 title에 현재 canonical tick과 state hash prefix가 나타난다.

## Quality gates

```powershell
cargo metadata --format-version 1 --no-deps --locked --offline
cargo check --workspace --all-targets --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline -- --test-threads=1
powershell -NoProfile -File .\scripts\stage0-technical-gate.ps1
cargo test -p aon-sim --test signal_conformance --locked --offline
cargo test -p aon-sim --test feedback_conformance --locked --offline
cargo test -p aon-sim --test replay_golden --locked --offline
cargo test -p aon-sim replay::tests --locked --offline
cargo test -p aon-headless --test replay_cli --locked --offline
cargo test -p aon-app --test replay_host --locked --offline
cargo test -p aon-fuzz-harness --locked --offline
```

cross-host test는 0, 1, 100 Tick의 기본 loop와 retained feedback 및 100,000-Tick Replay에서
Headless와 실제 Bevy `FixedUpdate`의 전체 hash trace를 비교한다. presentation update 수,
presenter 유무와 frame-delta partition도 Replay 결과를 바꾸지 않는다. Native window 실행은
display server가 필요한 수동 smoke gate다.

## Current API boundary

현재 Core API는 versioned `SimulationContract`, 실제 Numeric/Physical/Balance Profile,
fixed-point numeric/geometry, stable identity registry, canonical Stage 0 structural command,
Track Graph/TrackPosition/Mobile, Driver/Sink signal state, deterministic event calendar, 명시적
12-Phase `Simulation::step`, render snapshot, canonical state hash와 Replay v1까지 제공한다.

- Scenario, Numeric, Physical schema `1`과 Balance schema `2`, semantics
  `aon-semantics-v1`, hash algorithm `blake3-v1`을 현재 지원한다.
- Profile hash는 versioned canonical encoding을, state hash는 TrackPosition과 Mobile control
  mapping까지 포함한 `AON\0STATE\0V4\0` encoder를 사용한다. retained V3 Replay는 strict
  decode되지만 V4-only 실행에서는 typed unsupported-version 오류를 낸다.
- artifact의 Initial World는 아직 Empty만 지원하지만, command로 Fixed Substrate, Gate, Wire,
  Junction을 생성·바인드·제거할 수 있다.
- Phase 0은 ordinal ordering, deterministic rejection, geometry validation, generation/revision,
  fatal-overflow rollback을 구현한다.
- Phase 1/3/6/7/11은 Mobile 시작 위치와 world point snapshot, STOP/LEFT/RIGHT 단일 sample,
  정확한 movement budget, staged trajectory, EntityId 순서 commit을 구현한다. Phase 2/3/6은
  DriverTransition, SignalArrival/TopologySync, simultaneous Sink resolution, Gate truth table,
  inertial cancellation, transport delay를 구현한다.
- in-flight topology edit는 stamped Path Certificate로 검증한다. Route Diff가 만든 sync와
  propagation은 revision-aware slot comparison으로 합쳐지고, 제거된 route는 passive Low를
  즉시 재해결한다.
- Replay v1은 immutable initial-state Header, zero Seed/empty generator, 여덟 Stage 0
  Command variant, strict JSON, normalized Tick scheduling, sparse hash checkpoint를 제공한다.
- Headless는 `replay <path>`로 artifact 기준 상대 Scenario를 로드하고, Bevy harness는 같은
  Command Log를 canonical Tick 기준 `FixedUpdate`에 제출한다.
