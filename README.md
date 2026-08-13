# A/O/N

A/O/N은 AND, OR, NOT과 하나의 범용 Wire로 감지, 이동, 물류, 방어와 계산을 구성하는 결정론적 시뮬레이션 게임이다.

**Stage 0 Emergence Probe 전체가 정식 완료됐다.** S0-M6 Bevy ASCII Probe와 S0-M7
Mobility는 커밋 `bf651c9e63aed41e6db7e0f7b2767345834ff94e`의 독립 Windows-native
fresh clean-checkout offline 게이트와 실제 GUI 스모크를 통과했고, Stage 0 product gate는
2026-08-12 사용자 직접 A/B 플레이에서 PASS했다. 전체 엔진은 아직 완료되지 않았으며, 현황은
`docs/AON_Game_Engine_Implementation_Tracker_v1.0.md`에서 추적한다.

**S1-M0 Physical Scale Experiment Baseline도 정식 완료됐다.** 구현 커밋
`fe616fc6d9ffc81fd37864cf3d018343b327106b`는 별도 Windows-native clean clone에서 전체
workspace 검사와 테스트, Stage 0 게이트, 47-test S1-M0 게이트, 8-profile/16-run 실제
materialization을 모두 통과했다.

**S1-M1 Main Core / Capacity Accounting도 정식 완료됐다.** 구현 커밋
`5554f76266467d9112acdb2bad3ba5fcba4ed011`은 별도 Windows-native `git clone --no-local`
검증에서 514개 workspace 테스트와 Stage 0, S1-M0, 45-test S1-M1 게이트를 모두 통과했다.

**S1-M2 Sensing / Power / Brownout도 정식 완료됐다.** 구현 커밋
`22d6ccd89cb0e1fa422111f98f99c9d371122695`은 별도 Windows-native `git clone --no-local`
검증에서 629개 workspace 테스트와 25-test Stage 0, 47-test S1-M0, 45-test S1-M1,
70-test S1-M2 fail-closed 기술 게이트를 모두 통과했다.

**S1-M3 Capacity Support Load도 정식 완료됐다.** 구현 커밋
`f59fe50b6b19af0696e4f4fd0e2523f12889f973`은 별도 Windows-native `git clone --no-local`
검증에서 653개 등록 workspace 테스트와 25-test Stage 0, 47-test S1-M0, 45-test S1-M1,
70-test S1-M2, 29-test S1-M3 fail-closed 기술 게이트를 모두 통과했다. WSL은 사용하지
않았다.

**S1-M4 Construction / Contact / Damage는 구현 및 pre-commit 검증을 완료했다.** Balance v5,
Scenario v4, State V7, Construction Site/BUILD, canonical Enemy, Live Wire contact, Heat/Damage,
Phase-10→Phase-0 destruction과 terminal RunStatus가 구현됐고, 5개 retained Replay의 Headless/Bevy
전체 report·V7 trace가 일치한다. 95-test fail-closed S1-M4 게이트는 executable Gates 1–15를
통과했다. 정식 완료 선언과 S1-M5 진입은 구현 커밋의 별도 Windows-native clean clone에서
Gate 16까지 통과한 뒤 한다. WSL은 사용하지 않는다.

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
- S1-M0 implementation authority: `docs/AON_S1_M0_Canonical_Decisions_v1.0.md`
- S1-M1 implementation authority: `docs/AON_S1_M1_Canonical_Decisions_v1.0.md`
- S1-M2 implementation authority: `docs/AON_S1_M2_Canonical_Decisions_v1.0.md`
- S1-M3 implementation authority: `docs/AON_S1_M3_Canonical_Decisions_v1.0.md`
- S1-M4 implementation authority: `docs/AON_S1_M4_Canonical_Decisions_v1.0.md`

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

Retained S1-M1 Main Core capacity Replay:

```powershell
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m1-capacity-accounting-v1.json
```

Retained S1-M2 sensing and brownout Replays:

```powershell
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m2-c07-sensing-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m2-c08-brownout-full-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m2-c08-brownout-half-v1.json
```

Retained S1-M3 C-22 capacity-support Replay:

```powershell
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m3-c22-capacity-support-v1.json
```

이 fixture는 `U=120`, `S=100`, `E=20`, total Support Demand `28`, 낮은/높은 WireId의
share `17/11`, ordinary intrinsic demand `240`, Source generation `268`, Phase 8 Support Heat
`4+3`을 고정한다. State V7 trace와 전체 report는 Headless와 Bevy에서 동일해야 한다.

Retained S1-M4 Construction / Contact / Damage Replays:

```powershell
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m4/construction-partial-multibuilder-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m4/construction-four-targets-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m4/c10-contact-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m4/c09-wire-break-v1.json
cargo run -p aon-headless --locked --offline -- `
  replay fixtures/replays/s1-m4/terminal-v1.json
```

이 fixture set은 partial/source-less/full Construction, 4종 Site의 다음-Phase-0 fresh-ID 활성화,
C-10의 `20 -> 5+5+10`, C-09의 lethal Tick 45·전면 제거 Tick 46·stale Arrival Tick 51,
Main Core terminal Tick 55를 고정한다.

Materialize the retained S1-M0 Physical Scale experiment plan:

```powershell
cargo run -p aon-headless --locked --offline -- `
  experiment-plan fixtures/experiments/s1-m0-physical-scale-v1.json `
  --output target/s1-m0-materialized
```

The retained plan expands two explicit Gate geometries, two Circuit pitches, and two World
pitches into eight hash-sorted Physical Scale profiles. Two absolute long-wire distances reuse
those eight profile hashes and produce sixteen distinct canonical Run IDs. Distance is design
geometry, not a Physical Scale Profile field, and no profile or Module geometry is silently
scaled.

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
powershell -NoProfile -File .\scripts\s1-m0-technical-gate.ps1
powershell -NoProfile -File .\scripts\s1-m1-technical-gate.ps1
powershell -NoProfile -File .\scripts\s1-m2-technical-gate.ps1
powershell -NoProfile -File .\scripts\s1-m3-technical-gate.ps1
powershell -NoProfile -File .\scripts\s1-m4-technical-gate.ps1
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
12-Phase `Simulation::step`, render snapshot, canonical state hash와 Replay v2까지 제공한다.

- Scenario schema `1`의 Empty, schema `2`의 Main Core, schema `3`의 Main Core + Power Source,
  schema `4`의 Main Core + Power Source + canonical Enemy initial world, Numeric/Physical schema
  `1`, Balance schema `2`/`3`/`4`/`5`, semantics
  `aon-semantics-v1`, hash algorithm `blake3-v1`을 지원한다.
- Profile hash는 versioned canonical encoding을, state hash는 Main Core와 파생 anchor까지
  포함하고 Power Source, Gate retention, Wire Sense state를 추가한 전역
  `AON\0STATE\0V7\0` encoder를 사용한다. retained V3/V4/V5/V6 header는 strict
  decode되지만 V7 실행에서는 typed unsupported-version 오류를 낸다.
- Main Core는 첫 canonical EntityId, 위치, Capacity, Integrity, HeatEnergy와 implicit
  `MainCoreAnchor`를 가지며 제거할 수 없다. OpenWorld Wire만 정확한 anchor에 바인드할 수
  있고, 이 물리 endpoint는 Signal net을 서로 합치지 않는다.
- Phase 0은 ordinal ordering, deterministic rejection, geometry validation, generation/revision,
  fatal-overflow rollback을 구현한다.
- Phase 1/3/6/7/11은 Mobile 시작 위치와 world point snapshot, STOP/LEFT/RIGHT 단일 sample,
  정확한 movement budget, staged trajectory, EntityId 순서 commit을 구현한다. Phase 2/3/6은
  DriverTransition, SignalArrival/TopologySync, simultaneous Sink resolution, Gate truth table,
  inertial cancellation, transport delay를 구현한다.
- Phase 4는 WireId 순서로 모든 live Wire body 길이를 정확히 한 번 합산해 raw Fixed Capacity
  사용량 `U`와 Main Core 지원량 `S`를 산출한다. 초과 사용은 관찰 가능한 soft limit이며
  구조 배치를 거부하지 않는다. `StepReport`와 read-only Network Analyzer가 같은 값을
  노출하지만 derived accounting은 State Hash에 들어가지 않는다.
- Balance v4에서 Phase 4는 `E=max(0,U-S)`와 exact rational soft-support curve를 한 번의 최종
  ceil로 계산하고, 낮은 WireId부터 remainder를 배분한다. 양의 Wire share는
  `DemandKind::OvercapacitySupport` intrinsic load로 전체 nominal set에 포함되며, 실제 grant의
  `supportHeatFraction`만 Phase 8 report-only Heat가 된다. v2/v3 accounting은 S1-M3 관찰을
  `None`으로 유지하고 v4의 활성 zero는 `Some(0)`으로 구분한다. 이 derived 값들은 V7에
  들어가지 않으며 Capacity 초과가 build reject, Wire 삭제, 직접 delay/damage를 만들지 않는다.
- Balance v5는 Construction/Contact/Damage 계수를 추가한다. BUILD Mobile은 Site별 Work를
  요청하고 실제 Construction grant만 적용하며, 완료된 Site는 다음 Phase 0에 fresh ID로
  활성화된다. Canonical Enemy trajectory와 실제 Wire Body의 swept contact는 Live Wire의 실제
  grant를 보존적으로 흡수/Heat로 분할한다. Phase 9가 Heat를 정확히 한 번 적산하고 Phase 10이
  Tick-start Temperature와 Electrical exposure로 Integrity를 동시에 줄인 뒤, 다음 Phase 0에
  destruction을 적용한다. Main Core 파괴 Tick은 State V7과 `RunStatus::Ended`를 끝까지 commit한다.
- S1-M2 Power는 SourceAnchor 기반 region, exact common-ratio solve, Gate/Sense/Movement
  brownout grant와 Phase 8 leakage/transmission heat report를 제공한다. HostileFrame은 Phase 1
  complete one-Tick input이며, Wire Sense A/B는 기존 Signal surface와 분리된다. Power/Sense
  Analyzer와 Headless/Bevy 보고서는 derived read-only 관찰이고 canonical state를 바꾸지 않는다.
- in-flight topology edit는 stamped Path Certificate로 검증한다. Route Diff가 만든 sync와
  propagation은 revision-aware slot comparison으로 합쳐지고, 제거된 route는 passive Low를
  즉시 재해결한다.
- Replay v2는 immutable initial-state Header, zero Seed, Empty/MainCore/MainCorePower 및
  MainCorePowerEnemy world generator, 기존 Command와 tag-8 `PlaceConstructionSite`,
  MainCore/PowerSource/Sense/BUILD endpoint, strict typed HostileFrame,
  normalized Tick scheduling, sparse hash checkpoint를 제공한다. Decode-only v1은 nonempty
  world input을 계속 거부한다.
- Headless는 `replay <path>`로 artifact 기준 상대 Scenario를 로드하고, Bevy harness는 같은
  Command Log를 canonical Tick 기준 `FixedUpdate`에 제출한다.
- S1-M0 Core는 strict Physical Scale matrix, versioned Experiment plan/Run identity, Scenario와
  long-wire Design artifact hash, strict absolute-geometry Module v1을 제공한다. Headless의
  `experiment-plan`은 선언된 artifact를 모두 검증한 뒤 canonical profile/Run 파일을
  임시 sibling directory에서 완성하고 한 번에 게시한다.
