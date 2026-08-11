# A/O/N — Engine Bootstrap Milestones

**Version:** v1.0 Draft
**Scope:** TRD `S0-M0 — Bootstrap`
**Target:** Stage 0 기능 구현을 시작할 수 있는 엔진 구성 완료
**Source Baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft
**Implementation Status:** Engine Bootstrap Complete (2026-08-11)

---

# 0. 목적

이 문서는 A/O/N의 게임 기능을 구현하기 전에 필요한 엔진 기반을 작업 가능한 단위로 나눈다.

이 문서에서 **엔진 구성 완료**는 다음 상태를 뜻한다.

```text
Rust Toolchain 고정
+ Cargo Workspace 구성
+ Pure Rust Canonical Core 생성
+ Headless Host 생성
+ Bevy Interactive Host 생성
+ 빈 Profile / Scenario Artifact 로드
+ 두 Host의 Empty World Hash 일치
+ Local Quality Gate와 CI 통과
```

완료 후 다음 작업은 TRD의 `S0-M1 — Contract / Numeric / Identity`다.

---

# 1. 범위 경계

## 1.1 포함

- Rust 2024 Workspace와 toolchain pin
- `aon-sim`, `aon-app`, `aon-headless` crate
- Core / Host dependency boundary
- 최소 `Simulation` 수명주기 API
- Empty World와 Empty Render Snapshot
- 빈 Profile / Scenario fixture
- Headless와 Bevy의 공통 package loading 경로
- Empty World canonical hash
- formatting, lint, test, cross-host smoke test
- CI

## 1.2 포함하지 않음

- Fixed-point 연산과 geometry
- Stable EntityId와 ConnectionGeneration
- Gate / Wire / Junction 배치
- Command ordering과 Structural Phase의 실제 동작
- Signal event, delay, feedback
- Mobility
- Capacity, Relay, Power, Heat, Damage
- 최종 UI, 아트, 오디오

위 항목은 각각 `S0-M1` 이후의 책임이다. Bootstrap에서 임시 gameplay 구현으로 선행하지 않는다.

---

# 2. 완료 상태

목표 디렉터리 구조:

```text
and-or-not/
├─ Cargo.toml
├─ Cargo.lock
├─ rust-toolchain.toml
├─ crates/
│  └─ aon-sim/
├─ apps/
│  ├─ aon-app/
│  └─ aon-headless/
├─ profiles/
│  ├─ numeric/
│  ├─ physical-scale/
│  └─ balance/
├─ fixtures/
│  ├─ scenarios/
│  └─ replays/
└─ .github/
   └─ workflows/
```

Dependency 방향:

```text
aon-app      ─────► aon-sim
aon-headless ─────► aon-sim

aon-sim      ─X──► bevy
aon-sim      ─X──► aon-app
```

---

# 3. 마일스톤

| ID | 결과 | 승인 증거 |
|---|---|---|
| EB-M0 | 문서·도구 기준선 확정 | version과 실행 환경 기록 |
| EB-M1 | Cargo Workspace 구성 | 전체 package `cargo check` 성공 |
| EB-M2 | Canonical Core Shell | Empty World 반복 hash 일치 |
| EB-M3 | 공통 Bootstrap Artifact | 두 Host가 같은 fixture 로드 |
| EB-M4 | Headless Host 실행 | N tick 실행과 hash 출력 |
| EB-M5 | Bevy Host 실행 | Empty Snapshot 표시 |
| EB-M6 | Cross-host 결정론 검증 | tick hash sequence 일치 |
| EB-M7 | 자동 품질 Gate 구성 | clean checkout CI 통과 |

## EB-M0 — Baseline 확정

### 목표

구현이 참조할 문서와 개발 환경의 기준선을 명시한다.

### 작업

- PRD, SSS, TRD의 기준 version을 루트 README에 기록
- Rust compiler channel과 exact version 선택
- Bevy `0.19.x`의 exact version 선택
- 지원 Host를 우선 Native 하나로 제한
- 초기 CI 실행 환경 선택
- 현재 문서의 `:Zone.Identifier` 파일을 source artifact에서 제외

### 완료 조건

- 새 개발자가 어떤 문서와 toolchain을 기준으로 작업하는지 한 곳에서 확인 가능
- `rustc --version`과 `cargo --version` 기대값이 명시됨
- dependency version이 범위 표현이 아니라 lockfile로 재현됨

### 차단 조건

- 저장소가 유효한 Git 작업 트리가 아님
- 선택한 Rust toolchain에서 Bevy baseline이 빌드되지 않음

---

## EB-M1 — Cargo Workspace 구성

### 목표

세 crate가 하나의 Workspace에서 빌드되는 최소 구조를 만든다.

### 작업

- 루트 `Cargo.toml` 생성
- `rust-toolchain.toml` 생성
- `Cargo.lock` 생성 및 추적
- `crates/aon-sim` library crate 생성
- `apps/aon-app` binary crate 생성
- `apps/aon-headless` binary crate 생성
- workspace 공통 edition, lint, dependency 정책 정의
- `aon-sim`에 `#![forbid(unsafe_code)]` 적용

### 완료 조건

```bash
cargo metadata --no-deps
cargo check --workspace
```

두 명령이 성공하고 세 package가 모두 Workspace member로 나타난다.

---

## EB-M2 — Canonical Core Shell

### 목표

게임 규칙이 없는 상태에서도 Host가 사용할 안정적인 Core 경계를 만든다.

### 작업

- 최소 `Simulation`, `SimulationPackage`, `StepReport` 정의
- `Simulation::new`, `step`, `write_render_snapshot`, `state_hash` API 골격 정의
- `step()` 한 번이 정확히 한 canonical tick을 증가시키도록 구현
- wall-clock, Bevy, OS API 의존 금지
- Empty World canonical encoding과 hash 구현
- `RenderSnapshot`을 read-only projection으로 정의
- malformed empty package를 panic이 아닌 typed error로 반환

### 완료 조건

- `aon-sim` dependency tree에 Bevy 계열 crate가 없음
- 동일 Empty Package로 생성한 두 Simulation의 모든 tick hash가 일치
- render snapshot을 생성해도 canonical state hash가 바뀌지 않음
- Core test가 wall-clock이나 frame delta를 사용하지 않음

### 주의

이 단계의 hash encoder는 Empty World를 검증하는 최소 골격이다. Profile canonical hash, Fixed-point, EntityId encoding은 `S0-M1`에서 확장한다.

---

## EB-M3 — Bootstrap Artifact

### 목표

두 Host가 같은 입력 artifact로 같은 Simulation을 만들게 한다.

### 작업

- 빈 Numeric Profile fixture
- 빈 Physical Scale Profile fixture
- 빈 Balance Profile fixture
- Empty Scenario fixture
- 최소 schema/version 필드 정의
- fixture loading과 오류 메시지 정의
- Host별로 별도 default world를 생성하지 않고 공통 loader 사용

### 완료 조건

- Headless와 Bevy가 동일한 fixture path 또는 동일 bytes를 사용
- 누락되거나 잘못된 fixture를 deterministic error로 거부
- 현재 bootstrap fixture가 임시 artifact임을 schema/version으로 식별 가능

---

## EB-M4 — Headless Host

### 목표

창 없이 Simulation 생성, tick 실행, hash 출력을 수행한다.

### 작업

- `scenario` command의 최소 CLI 제공
- scenario path와 tick count 입력 지원
- 실행한 tick과 최종 state hash 출력
- Core error를 안정적인 process exit code로 변환
- Host에서 canonical state를 직접 변경할 수 없도록 API 사용

### 완료 조건

개념적 실행 계약:

```bash
cargo run -p aon-headless -- scenario fixtures/scenarios/empty.json --ticks 1
```

결과에 최소 다음이 포함된다.

```text
scenario = empty
completed_ticks = 1
state_hash = <canonical hash>
```

동일 명령을 반복하면 같은 hash가 출력된다.

---

## EB-M5 — Bevy Interactive Host Shell

### 목표

Bevy가 Canonical Core를 소유하지 않고 관찰·구동하는 첫 Host가 되게 한다.

### 작업

- Native Bevy window 생성
- `SimulationHostPlugin`과 최소 presenter 구성
- `CanonicalSimulation` resource wrapper 정의
- Canonical Simulation mutable owner를 한 system으로 제한
- FixedUpdate에서 Core `step()` 호출
- Update에서 `RenderSnapshot` 읽기
- Empty CellBuffer 또는 명확한 Empty World 화면 표시
- 종료 시 또는 debug panel에서 current tick/hash 노출

### 완료 조건

- Window가 열리고 Empty Render Snapshot이 표시됨
- 렌더링 FPS 변화가 canonical tick 결과를 바꾸지 않음
- presenter를 비활성화해도 Simulation 결과가 동일함
- Bevy `Entity` 또는 `Transform`이 canonical hash에 포함되지 않음

### 비목표

- Gate/Wire glyph
- Waveform
- Inspector
- Camera polish
- 최종 ASCII renderer

이들은 `S0-M6 — Bevy ASCII Probe`의 책임이다.

---

## EB-M6 — Cross-host Determinism Smoke Test

### 목표

Core / Host 분리가 실제 테스트로 강제되게 한다.

### 작업

- Empty Scenario를 0, 1, N tick 실행하는 shared test fixture 작성
- Headless 실행 결과와 Bevy host harness 결과 비교
- tick별 hash sequence 비교
- 서로 다른 presentation update 횟수로 같은 Core tick 수 실행
- 실패 시 첫 divergence tick을 출력

### 완료 조건

```text
같은 Empty Package
+ 같은 Tick Count

Headless Tick Hash Sequence
=
Bevy Tick Hash Sequence
```

최소 `0`, `1`, `100` tick fixture가 통과한다.

---

## EB-M7 — Quality Gate와 CI

### 목표

Bootstrap의 구조적 불변식이 이후 구현에서 자동으로 회귀 검증되게 한다.

### 작업

- formatter, clippy, workspace test CI 구성
- `aon-sim` dependency boundary 검사
- cross-host smoke test 실행
- fixture path가 working directory에 우연히 의존하지 않도록 검사
- CI cache는 사용 가능하지만 결과 artifact에는 영향을 주지 않게 구성

### 필수 Gate

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo tree -p aon-sim
```

### 완료 조건

- clean checkout에서 전체 Gate 성공
- `aon-sim` dependency graph에 Bevy 없음
- Empty World cross-host hash test 성공
- warning을 허용한 상태로 완료 처리하지 않음

---

# 4. 최종 승인 Gate

다음 항목이 모두 충족되면 **Engine Bootstrap Complete**로 판정한다.

## Architecture

- [x] Rust toolchain과 dependency가 lock됨
- [x] 세 crate가 Workspace에서 독립적으로 빌드됨
- [x] `aon-sim`은 Pure Rust이며 Bevy dependency가 없음
- [x] Bevy Host와 Headless Host가 동일한 public Core API를 사용함
- [x] Host가 canonical store에 대한 mutable reference를 얻지 못함

## Runtime

- [x] Empty Package로 Simulation 생성 가능
- [x] `step()` 한 번이 정확히 한 tick을 진행함
- [x] Empty Render Snapshot 생성 가능
- [x] Headless에서 N tick 실행 가능
- [x] Bevy window에서 Empty World 표시 가능

## Determinism

- [x] 동일 Empty Package의 hash가 반복 실행에서 일치함
- [x] Headless와 Bevy의 tick hash sequence가 일치함
- [x] snapshot 생성 횟수와 render FPS가 hash에 영향을 주지 않음

## Delivery

- [x] formatting, lint, test가 모두 통과함
- [x] CI가 clean checkout에서 통과함
- [x] 실행 방법과 crate 책임이 README에 기록됨
- [x] `S0-M1`이 추가 구조 재작업 없이 시작 가능함

---

# 5. 권장 실행 순서

```text
EB-M0 Baseline
    ↓
EB-M1 Workspace
    ↓
EB-M2 Core Shell ─────┐
    ↓                 │
EB-M3 Artifact        │
    ↓                 │
EB-M4 Headless        │
    ↓                 │
EB-M5 Bevy Host       │
    └────────┬────────┘
             ↓
EB-M6 Cross-host Determinism
             ↓
EB-M7 Quality Gate / CI
             ↓
ENGINE BOOTSTRAP COMPLETE
             ↓
S0-M1 Contract / Numeric / Identity
```

`EB-M2` 이후 Headless와 Bevy Host 작업은 일부 병행할 수 있다. 최종 완료 판정은 반드시 `EB-M6`의 cross-host hash equality 뒤에 한다.

---

# 6. 주요 리스크와 가드레일

| 리스크 | 초기 징후 | 가드레일 |
|---|---|---|
| Bevy가 canonical state를 소유함 | Core type이 Bevy Component가 됨 | `aon-sim` Bevy dependency 금지 |
| Bootstrap이 gameplay 구현으로 팽창함 | Gate/Wire 임시 로직 추가 | S0-M1 이후 항목은 명시적으로 보류 |
| 두 Host가 서로 다른 world를 만듦 | 각 binary에 default 상수 존재 | 공통 package/fixture loader 사용 |
| Hash가 구현 세부사항에 의존함 | map 순서나 pointer 값 encoding | 명시적 canonical encoder만 사용 |
| FPS가 tick 결과를 바꿈 | frame delta가 Core API로 전달됨 | Host pacing과 Core tick 완전 분리 |
| CI만 성공하고 앱이 실행되지 않음 | window smoke test 부재 | EB-M5 수동 실행 Gate 유지 |

---

# 7. 다음 마일스톤으로의 인계

Engine Bootstrap 완료 후 `S0-M1`은 다음 순서로 시작한다.

```text
Simulation Contract / Profile validation
→ Canonical numeric newtype
→ Division / sqrt / geometry
→ Stable EntityId / ConnectionGeneration
→ Canonical hash encoder 확장
→ C-17 Numeric Geometry
```

Bootstrap에서 만든 Core API, fixture loader, 두 Host, hash smoke test와 CI는 이후 모든 Stage의 고정 기반으로 유지한다.
