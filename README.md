# A/O/N

A/O/N은 AND, OR, NOT과 하나의 범용 Wire로 감지, 이동, 물류, 방어와 계산을 구성하는 결정론적 시뮬레이션 게임이다.

**Engine Bootstrap (`S0-M0`)은 완료됐다.** 다음 구현 단계는 `S0-M1 — Contract / Numeric / Identity`다. Gate, Wire, Signal 같은 gameplay semantics는 아직 구현하지 않는다.

## Source baseline

- Product: `docs/prd-v.1.0.md` — v1.0 GO Candidate
- Semantics: `docs/AON_Simulation_Semantics_Spec_v1.0.md` — v1.0 Draft
- Architecture: `docs/AON_TRD_v1.0.md` — v1.0 Draft
- Current milestones: `docs/AON_Engine_Bootstrap_Milestones_v1.0.md` — v1.0 Draft

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
cargo metadata --format-version 1 --no-deps --locked
cargo check --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

CI의 cross-host test는 0, 1, 100 tick에서 Headless loop와 실제 Bevy `FixedUpdate` schedule의 전체 hash trace를 비교한다. Native window 실행은 display server가 필요한 수동 smoke gate다.

## Bootstrap API boundary

현재 Core API는 `Simulation::new`, empty-only `Simulation::step`, render snapshot, state hash까지만 제공한다.

- Bootstrap profile은 schema `0`과 `bootstrap-*` ID로 실제 Stage 0 profile과 구분한다.
- Bootstrap state hash는 임시 domain/version을 사용한다.
- 실제 Simulation Contract, numeric profile, fixed-point geometry와 identity는 `S0-M1`에서 구현한다.
- 실제 Command ordering과 12-phase structural runtime은 `S0-M2` 이후 구현한다.
