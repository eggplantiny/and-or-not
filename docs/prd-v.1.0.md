# A/O/N — Product Requirements Document

**Full Title:** AND OR NOT
**Short Title:** A/O/N
**Version:** v1.0 GO Candidate
**Genre:** Programmable Infrastructure Survival / Tower Defense / Digital Logic Sandbox / Automation / Roguelike
**Development Context:** 1인 사이드 프로젝트
**Product Status:** **GO Candidate — Stage 0 implementation may begin**

---

# 0. 제품 정의

> **AND, OR, NOT 세 종류의 계산 소자와 하나의 범용 Wire만을 이용해 감지·공격·이동·물류·수리·계산을 구성하고, 제한된 물리 인프라 안에서 점점 더 지능적인 방어 문명을 만들어 Core가 파괴될 때까지 살아남는다.**

A/O/N은 완성된 Tower, Sensor, Robot, Memory, CPU, Weapon을 지급하는 게임이 아니다.

게임이 제공하는 것은:

* 작은 Primitive 집합
* 제한된 Energy
* 제한된 Network Capacity
* 실제 공간
* 시간
* 지속적으로 증가하는 외부 압력

뿐이다.

플레이어가 무엇을 만드는지는 그 조합의 결과다.

---

# 1. 제품의 핵심 판타지

A/O/N의 발전은 Tech Tree가 아니라 문제 규모의 증가에서 발생한다.

초기에는:

```text
Sensor
  ↓
AND
  ↓
Powered Wire
```

정도의 구조면 충분하다.

그러나 World가 커지면 단순한 물리 복제로 문제를 해결하기 어려워진다.

```text
더 많은 Sensor
더 많은 전용 Wire
더 많은 Track
더 많은 Attack Line
```

을 계속 복제하는 대신 플레이어는 점차:

```text
Feedback
State
Timing
Encoding
Multiplexing
Register
Memory
Scheduling
FSM
Programmable Computation
```

을 사용하게 된다.

A/O/N의 핵심 progression은:

> **더 많은 Infrastructure를 만드는 것에서, 같은 Infrastructure를 더 지능적으로 사용하는 것으로 이동하는 과정**

이다.

---

# 2. 제품의 핵심 질문

A/O/N은 세 가지 질문을 게임으로 만든다.

### Q1

> **AND, OR, NOT만으로 어디까지 복잡한 시스템을 만들 수 있는가?**

### Q2

> **문제를 물리적으로 복제해서 해결할 것인가, 계산으로 압축해서 해결할 것인가?**

### Q3

> **같은 Energy와 같은 Network에서 얼마나 많은 유효 행동을 만들어낼 수 있는가?**

---

# 3. 핵심 제품 가설

## H1 — Emergence

> **충분히 작은 Primitive 집합에서도 개발자가 별도 구현하지 않은 유용한 기계와 행동이 등장할 수 있다.**

게임에는 다음 Gameplay Class가 없어야 한다.

```text
RepairBot
CombatDrone
Counter
Memory
CPU
Router
Multiplexer
RoutePlanner
FireControlComputer
BeamTower
```

그러나 플레이어가 만든 Circuit은 실제 World에서 그러한 역할을 수행할 수 있어야 한다.

> **창발 자체가 콘텐츠다.**

---

## H2 — Computation substitutes Infrastructure

> **문제 규모가 커질수록 더 많은 계산을 사용하는 설계가 물리 Infrastructure를 단순 복제하는 설계보다 더 높은 확장 효율을 얻을 수 있어야 한다.**

예:

```text
Brute Force

16 Sensor
16 Long Signal Lines
Minimal Logic
```

대신:

```text
Computed

16 Sensor
Local Logic
Encoding
4 Long Signal Lines
```

또는:

```text
Computed Further

Sparse Sensor
Temporal State
Prediction
Few Long Signal Lines
```

이 가능해야 한다.

---

## H3 — Computation advantage emerges with scale

Brute Force를 처음부터 틀린 해법으로 만들지 않는다.

이상적인 progression은 다음이다.

```text
EARLY
Physical Replication 우세 또는 충분

        ↓

MID
Physical Replication
≈
Computation

        ↓

LATE
Computation의 확장 효율 우세
```

초반에는 Wire를 더 까는 것이 가장 빠르고 쉬운 선택일 수 있다.

문제 규모가 커질수록:

* Network Capacity
* Power
* Delay
* Heat
* Construction Time
* Repair Burden
* Exposure

때문에 계산의 경제적 가치가 커진다.

---

# 4. 절대적 제품 불변식

1. 플레이어가 채굴하는 계산 자원은 `A / O / N`뿐이다.
2. 계산 Primitive는 `AND / OR / NOT`뿐이다.
3. 새로운 기능이 필요하다는 이유만으로 새로운 계산 Primitive를 추가하지 않는다.
4. Wire는 A/O/N World의 범용 물리 Primitive다.
5. 모든 Circuit은 실제 Gate와 Wire Layout으로 존재한다.
6. Circuit 내부 Wire 역시 실제 Wire이며 Network Capacity를 사용한다.
7. Module은 새 Component가 아니라 실제 Layout의 Blueprint다.
8. Circuit은 하나의 의미 Class를 갖지 않는다.
9. 상위 Circuit은 직접적인 Stat Bonus를 받지 않는다.
10. Signal은 시간에 따른 Waveform이다.
11. Feedback을 허용한다.
12. 기술트리는 존재하지 않는다.
13. 정보는 무료가 아니다.
14. Infrastructure는 무료가 아니다.
15. Wire의 Material Cost는 0일 수 있지만 Opportunity Cost는 0이 아니다.
16. Wire와 Computation은 일부 문제에서 대체재가 되어야 한다.
17. Geometry와 Timing은 Energy를 재분배할 수 있지만 생성할 수 없다.
18. 상위 계산 구조는 하위 구조의 완전한 상위호환이 아니다.
19. 동일한 Simulation 입력은 Host 성능과 무관하게 동일한 결과를 만든다.
20. World에서 시간은 계속 흐른다.
21. Run 종료 시 물리 세계는 사라진다.
22. Module Library만 다음 Run으로 계승된다.

---

# 5. World Ontology

## 5.1 World Infrastructure

맵에 원래 존재하며 플레이어가 임의 위치에 복제할 수 없다.

* Main Core
* Relay Site
* Power Source
* A Deposit
* O Deposit
* N Deposit
* Quartz
* Terrain
* Enemy

---

## 5.2 Construction Primitives

플레이어가 배치할 수 있다.

* Wire
* Junction
* Fixed Substrate
* Mobile Substrate

Construction Primitive는 희소 Material Inventory를 요구하지 않을 수 있다.

그러나 다음을 소비한다.

* Network Capacity
* Construction Time
* Electricity
* Space
* Leakage
* Heat
* Physical Exposure

> **무료지만 공짜가 아니다.**

---

## 5.3 Computational Primitives

```text
A = AND
O = OR
N = NOT
```

이 세 종류만 직접 채굴하고 소비한다.

---

# 6. Main Core

Run은 하나의 Main Core에서 시작한다.

Main Core는:

* Run 종료 조건
* 초기 Network Capacity
* 초기 Network Anchor

를 제공한다.

```text
MAIN CORE DESTROYED
→ RUN END
```

Main Core는 복제할 수 없다.

---

# 7. Relay

Relay는 맵에 고정 배치되는 점령형 World Resource다.

> **Relay는 활성화되면 추가 Network Capacity를 제공한다.**

Relay는 계산하지 않는다.

다음 기능을 제공하지 않는다.

```text
Automatic Routing
Signal Processing
Pathfinding
Targeting
Power Priority
Automatic Buffering
```

이러한 행동은 모두 A/O/N Circuit의 책임이다.

---

# 8. Relay Site

Relay는 아무 위치에나 제작할 수 없다.

World Generation 시 Relay Site의 위치가 정해진다.

개념적으로:

```text
Relay Site
    ↓
Network 연결
    ↓
Power 공급
    ↓
Activation
    ↓
Relay Online
```

Relay Structure는 Enemy 공격으로 파괴될 수 있다.

Relay Site 자체는 남는다.

따라서:

```text
Relay destroyed
→ Reconstruction 가능

새로운 Relay Site 생성
→ 불가능
```

이다.

---

# 9. Global Network Capacity

v1.0의 Network Capacity는 **Global Pool**을 사용한다.

```text
Total Network Capacity

=
Main Core Capacity
+
Σ Online Relay Capacity
```

Regional Capacity는 v1.0 범위에서 사용하지 않는다.

이유:

* Backbone Capacity 귀속 규칙을 추가하지 않는다.
* Relay 경제의 핵심 가설을 먼저 단순하게 검증한다.
* 모든 Physical Wire를 한 번만 계산한다.

향후 지리적 압력이 부족하다는 플레이 증거가 생기면 Regional Capacity를 별도 revision으로 검토한다.

---

# 10. Network Capacity 소비

모든 Wire는 실제 Physical Length에 따라 Capacity를 소비한다.

개념적으로:

```text
Network Usage
=
Σ Physical Wire Length
```

다음 Wire를 구분하지 않는다.

```text
World Backbone
Circuit Internal Wire
Sensor Wire
Track Wire
Radiation Wire
```

모두 실제 Wire다.

---

# 11. Multi-role Wire Accounting

하나의 Wire는 동시에:

```text
Signal
Power
Sensing
Mobility
```

역할을 할 수 있다.

그러나 Network Capacity는 역할 수에 따라 중복 계산하지 않는다.

같은 Physical Wire는 실제 길이만큼 **한 번만** 계산한다.

따라서 여러 역할을 한 Wire에 통합하는 설계는 실제 Network Efficiency를 높인다.

---

# 12. Network Capacity는 Soft Support Limit다

Network Capacity는 절대적인 Build Permission이 아니다.

예:

```text
Capacity = 1000
Installed Wire = 1050
```

라고 해서 Wire가 삭제되거나 즉시 건설이 거부되지 않는다.

대신 **Overcapacity** 상태가 발생한다.

Overcapacity는 Network 유지 Overhead를 증가시킨다.

그 결과 기존 시스템을 통해:

```text
Power Demand ↑
Leakage / Support Overhead ↑
Heat ↑
Brownout Risk ↑
Operational Margin ↓
```

가 발생한다.

정확한 함수는 Simulation Semantics에서 정의한다.

중요한 불변식:

> **Capacity 초과는 불가능 상태가 아니라 점점 비경제적인 상태다.**

---

# 13. Relay Loss

Relay 파괴로 Capacity가 감소해도 기존 Wire를 삭제하지 않는다.

예:

```text
Before

Supported = 1500
Used      = 1400

Relay destroyed

Supported = 900
Used      = 1400
```

이 경우 World는 계속 존재하지만 심각한 Overcapacity에 들어간다.

따라서 Relay는 단순 Expansion Token이 아니라 실제 전략적 Infrastructure가 된다.

---

# 14. Network Capacity의 진짜 목적

Network Capacity의 목적은:

> **Wire 도배를 금지하는 것**

만이 아니다.

더 중요한 목적은:

> **물리적 복제를 계산적 복잡성으로 치환할 경제적 이유를 만드는 것**

이다.

---

# 15. Computation ↔ Infrastructure Exchange

예를 들어 공간 정보를 얻고 싶다고 하자.

### Physical Replication

```text
Dense Sensor Grid
→ 높은 Sensor Resolution
→ 많은 Wire
```

### Computation

```text
Sparse Sensor
+
Previous State
+
Current State
+
Logic
→ Movement / Position 추정
```

---

통신에서도:

### Physical Replication

```text
16 Signals
→ 16 Long Lines
```

### Computation

```text
16 Signals
→ Local Encoding
→ 4 Long Lines
→ Decode
```

---

더 고급으로:

```text
Multiple Signals
→ Multiplexing
→ Shared Wire
→ Timing
→ Register
→ Demultiplexing
```

을 만들 수도 있다.

게임은 `Encoder`, `MUX`, `Serial Bus`를 Primitive로 제공하지 않는다.

---

# 16. Circuit Physical Scale

Circuit 내부 Wire도 Network Capacity를 소비하므로 Local Computation이 실제로 Long Wire를 대체하려면 물리 축척이 중요하다.

제품 요구:

> **Local Circuit을 구성하는 실제 Wire Length가 충분히 작아, 일정 규모 이상의 장거리 Infrastructure를 계산으로 대체할 수 있는 Physical Scale Regime이 존재해야 한다.**

PRD는 Gate 크기를 영구 상수로 고정하지 않는다.

대신 Stage 1 착수 전에 **Physical Scale Profile v0-alpha**를 고정한다.

포함 대상:

* Gate Minimum Footprint
* Circuit Routing Pitch
* World Routing Pitch
* Wire Geometry Unit

---

# 17. Physical Scale 검증

다음 예를 사용한다.

Brute Force:

```text
16 × Long Wire Length L

Cost = 16L
```

Computed:

```text
Local Circuit Wire W
+
4 × Long Wire Length L

Cost = W + 4L
```

Computed 설계가 Capacity를 절약하려면:

```text
W < 12L
```

인 영역이 실제 Layout에서 존재해야 한다.

이를 가정하지 않는다.

**Stage 1에서 실제로 측정한다.**

그러한 영역이 현실적인 Layout에서 존재하지 않는다면 H2는 실패다.

---

# 18. Wire

Wire는 A/O/N World의 중심 물리 Primitive다.

하나의 Wire Body가:

```text
Wire
├─ Signal
├─ Power
├─ Sensing
└─ Mobility
```

를 담당한다.

Wire가 길거나 많아질수록:

* Network Capacity
* Signal Delay
* Leakage
* Power Loss
* Heat
* Construction Time
* Damage Exposure

가 증가한다.

---

# 19. Sensing

Wire는 일정 반경의 Hostile Occupancy를 1bit로 제공한다.

```text
0 = 없음
1 = 있음
```

직접 제공하지 않는다.

* 정확한 좌표
* 정확한 거리
* 적 숫자
* 속도
* 방향
* HP
* Target

고해상도 정보는 더 많은 Wire 또는 더 많은 Computation으로 얻는다.

---

# 20. Electricity

Electricity는 Inventory 자원이 아니다.

실시간 Flow다.

```text
Generation
Demand
Reserve
```

Electricity는:

* Gate
* Wire
* Sensing
* Attack
* Movement
* Extraction
* Construction

의 근원 Energy다.

역할 구분:

```text
Power Source
→ 얼마나 많은 일을 동시에 할 수 있는가

Relay
→ 얼마나 큰 Network를 유지할 수 있는가

A/O/N
→ 그 Network를 얼마나 지능적으로 사용할 수 있는가
```

---

# 21. Brownout

Demand가 Generation을 초과하면 모든 시스템이 즉시 OFF되지 않는다.

Brownout은:

* Drive
* Timing
* Movement
* Build Rate
* Extraction
* Attack Output

을 악화시킨다.

Load Shedding은 플레이어가 Circuit으로 구성한다.

---

# 22. Heat

Heat는 활동의 부산물이다.

Source:

* Gate Switching
* Wire Leakage
* Power Transmission
* Movement
* Contact Attack
* Radiation

Heat는:

* Timing
* 안정성
* Damage Risk

에 영향을 준다.

Heat를 Enemy에게 전달하면 공격 Energy로 활용할 수 있다.

---

# 23. Damage Ontology

A/O/N의 Damage Type은 두 개뿐이다.

```text
Electrical
Thermal
```

그 외 개념은 Energy 전달 방식이다.

---

# 24. Contact Attack

Powered Wire가 Enemy와 접촉하면 Electrical / Thermal Energy가 전달될 수 있다.

가장 원시적인 Defense는 단순 Powered Wire다.

그러나:

```text
모든 Defense 항상 ON
```

은 큰 Power Cost를 만든다.

Circuit은:

```text
어디를
언제
얼마 동안
```

활성화할지 결정한다.

---

# 25. Intelligent Defense

같은 Physical Network에서도 더 좋은 Circuit은 훨씬 높은 효율을 만들어야 한다.

### Brute

```text
Enemy somewhere
→ All Defense ON
```

### Local

```text
Sector Presence
→ Sector Defense ON
```

### Stateful

```text
Previous Presence
+
Current Presence
→ Direction
→ Next Sector pre-activation
```

A/O/N의 실력 차이는 발전량 자체보다:

> **같은 Energy를 얼마나 선택적으로 사용하는가**

에서 발생해야 한다.

---

# 26. Radiation

장거리 공격을 위해 새로운 Weapon Primitive를 추가하지 않는다.

Radiation은:

> **Wire Drive의 Switching Energy가 Geometry와 Timing에 따라 공간으로 전달되는 결정론적 이산 Energy 전달**

이다.

```text
Switching
+
Geometry
+
Arrival Timing
→ Spatial Energy Distribution
```

Radiation은 새로운 Damage Type이 아니다.

---

# 27. Radiation과 Computation

단순한 Oscillator는 원시 Radiation을 만들 수 있다.

더 높은 계산은:

```text
Counter
Divider
Delay
Register
Sensor Processing
Timing Logic
```

을 이용하여 같은 Energy를 더 적절한 Cell과 Tick에 집중할 수 있다.

따라서:

```text
Radiation Wire 추가
```

와

```text
Timing Logic 개선
```

도 부분적인 대체재다.

---

# 28. Mobile Substrate

Mobile Substrate는 자유 이동 Robot이 아니다.

> **Wire Network를 따라 이동하는 Circuit Substrate**

다.

저수준 Port:

```text
STOP
LEFT
RIGHT
LOAD
UNLOAD
BUILD
```

만 제공한다.

다음은 제공하지 않는다.

```text
MOVE_TO
PATHFIND
REPAIR
DELIVER_TO
```

---

# 29. Mobility와 Computation

단순한 물류 시스템은 목적지마다 Direct Track을 만들 수 있다.

```text
A → B
A → C
A → D
```

더 지능적인 시스템은:

```text
Shared Track
+
Junction
+
Route State
```

를 사용한다.

즉 Routing Logic 역시 Network Capacity를 절약하는 계산 구조가 될 수 있다.

---

# 30. Damage와 Reconstruction

Gate와 Wire는 개별적으로 파괴된다.

Module 전체 HP는 없다.

Repair는 고수준 Action이 아니다.

```text
Fault Detection
↓
Task State
↓
Cargo 확보
↓
Route 이동
↓
BUILD
```

을 플레이어 Circuit이 수행한다.

---

# 31. Circuit Semantics

Signal은:

```text
Signal(t)
```

이다.

Gate와 Wire에는 Delay가 존재한다.

따라서:

* Pulse
* Oscillation
* Glitch
* Race
* Hazard

가 실제 World 행동으로 이어진다.

---

# 32. Feedback

Output을 Input 쪽으로 연결할 수 있다.

Feedback에서 자연스럽게:

* Oscillator
* Latch
* Flip-Flop
* Counter
* Register
* Memory
* FSM

과 유사한 구조가 만들어질 수 있다.

이들은 Gameplay Type이 아니다.

---

# 33. Circuit Capability

게임은 Circuit을:

```text
Memory
Counter
CPU
```

중 하나로 강제 분류하지 않는다.

Analyzer가 제공할 수 있는 것은:

* Stateful
* Periodic
* Stable
* Reachable States
* Footprint
* Wire Length
* Power
* Heat
* Delay

등 관찰 가능한 특성이다.

의미는 플레이어가 붙인다.

---

# 34. Module

Module은 실제 Circuit Layout 전체를 저장한 Blueprint다.

저장 대상:

* Gate 종류
* Gate 위치
* Wire Routing
* Junction
* I/O
* Physical Footprint
* Provenance

Module 배치 시 실제 Primitive가 다시 시공된다.

---

# 35. Module Optimization

같은 역할의 Module에도 다양한 Variant가 존재할 수 있다.

```text
compact
fast
low-power
low-wire
low-heat
low-N
```

특히:

```text
low-wire
```

최적화는 다음 Run에서 Network Capacity를 직접 절약한다.

따라서 Meta Progression은 Stat Bonus가 아니라 **더 좋은 설계의 축적**이다.

---

# 36. Computation Progression

다음은 Tech Tree가 아니라 자연스러운 활용 예다.

| 구조               | 해결하기 쉬워지는 문제           |
| ---------------- | ---------------------- |
| A/O/N            | 현재 조건                  |
| Latch            | 과거 사건                  |
| Flip-Flop        | 상태 전환                  |
| Counter          | 횟수 / 순서 / Timing       |
| Encoder          | Signal 압축              |
| MUX-like circuit | Wire 공유                |
| Shift Register   | Route / Sequence       |
| Register         | Snapshot / Target      |
| Memory           | Task / History / Table |
| FSM              | 반복 행동                  |
| CPU-like System  | 여러 정책의 데이터화            |

---

# 37. 중앙집중과 분산 계산

중앙집중:

```text
Many Sensor
→ Many Long Wire
→ Central Controller
```

분산:

```text
Local Sensor
→ Local Logic
→ Compressed State
→ Central Controller
```

둘 다 유효해야 한다.

Trade-off:

```text
Central
→ Logic 재사용 좋음
→ Backbone 비용 큼

Distributed
→ Backbone 절약
→ A/O/N과 local logic 비용 큼
```

---

# 38. Quartz

Quartz는 World에 존재하는 희소 Timing Infrastructure다.

Quartz는 Clock을 가능하게 하지 않는다.

Feedback Oscillator는 Quartz 없이 만들 수 있다.

Quartz는:

> **안정적인 Timing Reference**

를 제공한다.

---

# 39. A/O/N Structural Asymmetry

A/O/N 공급을 완전히 대칭적인 가치로 가정하지 않는다.

논리적으로 N은 A/O보다 대체하기 어렵고, A와 O는 N을 이용해 서로 비싸게 대체할 수 있다.

이제 대체 회로가 추가 Wire까지 요구하므로 Resource Asymmetry는 Network Capacity에도 영향을 줄 수 있다.

따라서:

> **A/O/N 공급 비율은 사전 미학으로 결정하지 않고 실제 회로 사용량과 대체 비용을 측정해 결정한다.**

Stage 1 이후 Telemetry로 판단한다.

초기에는 N을 인위적으로 희귀하게 만들지 않는다.

---

# 40. Enemy Philosophy

Enemy는 특정 Circuit의 Puzzle Key가 아니다.

```text
Enemy X → Counter 필요
Enemy Y → Radiation 필요
```

같은 규칙을 만들지 않는다.

Enemy는 다른 종류의 시스템 압력을 만든다.

---

# 41. MVP Enemy Set

## Assault

Core 방향으로 접근.

압력:

* Contact Defense
* Power
* Wire Geometry

## Ranged

접촉 방어선에 들어오지 않고 Infrastructure 공격.

압력:

* Radiation
* Forward Network
* Mobile Response
* Repair

## Drop / Artillery

내부 Infrastructure를 직접 공격.

중요 Target:

* Relay
* Power Source
* Backbone
* Controller

압력:

* Internal Sensing
* Redundancy
* Repair
* Routing

## Suicide

매우 빠른 접근.

압력:

* Local Logic
* Low Latency
* Redundancy

상위 CPU가 모든 문제의 정답이 되지 않게 한다.

---

# 42. Construction

Construction Primitive는 즉시 생성되지 않는다.

Gate와 Wire는 Construction Time을 요구한다.

긴 Wire는 더 많은 Work를 요구한다.

따라서 두 제약을 분리한다.

```text
Network Capacity
→ 얼마나 많이 유지할 수 있는가

Construction Throughput
→ 얼마나 빨리 확장할 수 있는가
```

---

# 43. Laboratory

Laboratory는 Run 외부의 Circuit Playground다.

지원:

* Pause
* Step
* Speed
* Reset
* Unlimited A/O/N
* Unlimited Electricity
* Signal Probe
* Waveform
* Analyzer
* Sensing Overlay
* Radiation Overlay
* Mobile Test Track

Circuit Layout과 Wire Length는 실제 Physical Scale을 유지한다.

---

# 44. Run과 Meta Progression

Run 종료 시 사라진다.

* A/O/N
* Wire Network
* Relay Activation
* Power Network
* Quartz
* Mobile / Fixed Substrate
* Installed Circuit
* Territory

남는 것:

> **Module Library**

뿐이다.

Permanent:

```text
Attack +10%
Starting Capacity +20%
Mining +30%
```

같은 Stat Bonus는 기본 Meta Progression으로 사용하지 않는다.

---

# 45. Infinite Survival

확장하면 더 많은:

* Relay
* Power
* Deposit
* Quartz

를 얻을 수 있다.

그러나 동시에:

* Backbone Length
* Defense Perimeter
* Relay Exposure
* Repair Distance
* Network Complexity

도 증가한다.

핵심 선택은:

> **새 Relay를 얻기 위해 확장할 것인가, 현재 Network를 계산으로 압축할 것인가?**

이다.

---

# 46. Progression Curve Requirement

A/O/N은 다음 곡선을 목표로 한다.

## Early

Brute Force가 정상적인 해법이다.

```text
Wire를 더 깐다
→ 문제 해결
```

빠르고 이해하기 쉽다.

---

## Mid

Brute와 Computed가 경쟁한다.

```text
More Wire
vs
More Logic
```

플레이어 스타일에 따라 둘 다 유효하다.

---

## Late

문제 규모가 커지며 Computed Design의 확장성이 우세해진다.

Brute Force가 금지되지는 않는다.

하지만:

* Capacity
* Power
* Construction
* Repair
* Delay
* Exposure

부담이 더 빠르게 증가한다.

---

# 47. Stage 0 — Emergence Probe

**질문:**

> **현재 Input만으로 해결할 수 없는 World 행동 때문에 State를 만들고 싶어지는가?**

구성:

```text
AND
OR
NOT
Wire
Junction
Mobile Substrate
Delay
Feedback
```

대표 Scenario:

```text
A 출발
↓
Junction 통과
↓
B 도착
↓
과거 조건 유지
↓
STOP / RETURN
```

### Stage 0 PASS

* Latch/FSM에 해당하는 행동을 별도 Runtime Class 없이 만들 수 있다.
* 실제 플레이에서 State를 만들 이유가 느껴진다.

### FAIL

State가 단순 퍼즐 장식에 불과하다면 Stage 1로 진행하지 않는다.

---

# 48. Stage 1 — Capacity Economy Probe

**질문:**

> **Computation이 실제로 Infrastructure를 대체하는가?**

필요 시스템:

```text
A/O/N
Wire
Core
Global Network Capacity
Sensing
Electricity
Contact Attack
Enemy 1종
```

불필요:

```text
Relay
Radiation
Repair
Payload
Quartz
Enemy 4종
```

---

# 49. Stage 1 Reference Scenario

동일한:

* Power
* Core Capacity
* Territory
* Enemy Sequence

를 사용한다.

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

비교:

* Survival
* Total Wire Length
* Gate Count
* Power
* Construction Work
* Heat
* Response Latency

---

# 50. Stage 1 PASS 조건

다음 세 영역이 모두 관찰되어야 한다.

### Early-scale regime

Brute Force가 충분히 유효하다.

### Crossover regime

Brute Force와 Computed가 실질적으로 경쟁한다.

### Large-scale regime

Computed Design이:

> **더 많은 Gate를 사용하더라도 더 적은 Network Capacity로 더 높은 확장 효율**

을 얻는다.

이 Crossover가 현실적인 Physical Scale에서 존재하지 않으면 H2는 실패한다.

Relay 구현 전에 원인을 수정한다.

---

# 51. Physical Scale Profile Gate

Stage 1 전에 `Physical Scale Profile v0-alpha`를 고정한다.

하나의 숫자를 감으로 선택하지 않는다.

최소 Parameter Sweep:

```text
Gate Footprint
Circuit Routing Pitch
Long-wire Distance
Network Capacity
```

를 변화시켜 Crossover 영역을 찾는다.

목표:

> **게임 규칙을 Computation 승리로 조작하는 것이 아니라, 두 전략이 실제로 교차하는 유효 설계 공간을 찾는다.**

---

# 52. Stage 2 — Relay Expansion Probe

Stage 1 통과 후 Relay를 구현한다.

**질문:**

> **Relay가 Network Expansion과 전략적 Territory 선택을 만드는가?**

구성:

```text
Main Core
Global Capacity
2~3 Relay Site
Relay Activation
Relay Destruction
Overcapacity
Simple Enemy Pressure
```

검증:

* Relay 확보가 실제 확장 동기가 되는가?
* Relay 상실이 Network Crisis를 만드는가?
* Relay를 얻는 것과 기존 Network 최적화가 경쟁하는가?

---

# 53. MVP — Emergent Defense Vertical Slice

Stage 0, 1, 2 통과 후 구현한다.

포함:

## World

* Main Core
* Relay
* Power Source
* A/O/N Deposit
* Quartz
* 4 Enemy Types

## Primitive

* A/O/N
* Wire
* Junction
* Fixed Substrate
* Mobile Substrate

## Systems

* Network Capacity
* Construction
* Sensing
* Power / Brownout
* Heat
* Contact Attack
* Radiation
* Mobility
* Payload
* Damage
* Reconstruction
* Module
* Laboratory

---

# 54. MVP 핵심 성공 장면

```text
Drop Enemy 착탄
↓
Backbone 파괴
↓
Local Circuit이 Fault 추론
↓
Task State 유지
↓
Mobile Substrate 출발
↓
Shared Network Routing
↓
A/O/N Cargo 운반
↓
BUILD
↓
Network 복구
```

엔진에는:

```text
RepairBot
FSM
Memory
RoutePlanner
```

라는 Gameplay Class가 존재하지 않는다.

---

# 55. MVP 성공 조건

### V1 — Emergence

예상하지 않은 유용한 행동이 Primitive 조합에서 나온다.

### V2 — State

과거 정보가 실제 World 행동에 가치를 가진다.

### V3 — Computation substitutes Infrastructure

문제 규모가 증가할수록 Computation이 Wire 복제보다 높은 Capacity 효율을 얻는다.

### V4 — Brute Force remains valid

작은 규모와 일부 상황에서는 단순 Infrastructure가 정상적인 선택이다.

### V5 — Crossover exists

Brute와 Computed 사이에 실제 경쟁 영역이 존재한다.

### V6 — No Blanket-Wire Dominance

단순 전맵 Wire 도배가 장기적 지배전략이 아니다.

### V7 — Capacity does not dominate everything

Network Capacity만 최소화하는 것이 유일한 최적화 목표가 아니다.

Power, Delay, Heat, Construction, Exposure와 실제 Trade-off가 존재한다.

### V8 — Relay creates expansion pressure

Relay 확보가 실제 전략적 확장을 만든다.

### V9 — Optimization matters

Module을 compact / low-wire / low-power하게 다시 만들고 싶어진다.

### V10 — Abstraction matters

기존 Module 재사용으로 더 높은 문제에 집중할 수 있다.

### V11 — Multiple solutions

동일 World Problem에 여러 유효한 Architecture가 존재한다.

---

# 56. Post-MVP Web Alpha

MVP가 최소한의 Game Loop를 형성하면 무료 Browser Build를 공개한다.

목적은 판매가 아니라 제품 검증이다.

무료 Web Alpha에서 확인할 핵심 질문:

> **설명을 듣지 않은 플레이어가 스스로 Circuit을 개선하고 싶어지는가?**

관찰할 대표 신호:

* 첫 Wire 배치
* 첫 Gate 사용
* 첫 Enemy Kill
* 첫 Feedback Circuit
* 첫 Module 저장
* Wave reached
* Session duration
* Total Wire / Gate ratio

가장 강한 정성적 신호는:

> **플레이어가 개발자가 의도하지 않은 Circuit이나 행동을 발견해 공유하는 것**

이다.

---

# 57. SSS 책임

Simulation Semantics Specification은 다음을 확정한다.

* Network Capacity Unit
* Wire Length Accounting
* Overcapacity Curve
* Relay Online / Offline
* Relay Destruction
* Physical Scale Numeric Profile
* Construction Work
* Gate / Wire Delay
* Power / Brownout
* Heat
* Sensing
* Radiation
* Mobility
* Damage
* Determinism

Stage 0은 기존 SSS v0.2 기반으로 구현을 시작할 수 있다.

Stage 1 착수 전 Network Capacity 관련 SSS revision이 필요하다.

---

# 58. TRD 책임

TRD는 다음 구현을 정의한다.

* Global Capacity Accounting
* Relay State Storage
* Topology Compile
* Event Queue
* Spatial Index
* SoA
* Deterministic Overcapacity Evaluation
* Replay
* Performance Optimization

Stage 0의 기존 Pure Rust Canonical Core / Bevy Host 분리 원칙은 유지한다.

---

# 59. GO 이후 남겨도 되는 Open Items

다음은 **Stage 0 구현을 막지 않는다.**

### OPEN-A — A/O/N Supply Balance

실제 사용량 측정 후 결정.

### OPEN-B — Exact Capacity Curve

Stage 1 전 SSS에서 확정.

### OPEN-C — Physical Scale Numeric Values

Stage 1 전 Parameter Sweep으로 결정.

### OPEN-D — Relay Capacity Amount

Stage 2 Balance 항목.

### OPEN-E — Future Regional Capacity

Global Capacity가 지리적 전략을 충분히 만들지 못한다는 플레이 증거가 있을 때만 검토.

### OPEN-F — Final World Scale

실제 Performance / Playtest 이후 결정.

---

# 60. 명시적으로 보류하는 것

현재 구현 범위에 포함하지 않는다.

* Regional Network Capacity
* Copper / Cable 등 Wire Material Resource
* 추가 Logic Primitive
* Automatic Sensor Device
* Automatic Router
* Wireless Control
* Battery / Capacitor
* Signal Crosstalk
* Maxwell Simulation
* Automatic Pathfinding
* Automatic Repair Planning
* Enemy-specific Required Circuit
* Permanent Stat Meta Progression

필요성이 실제 플레이에서 증명되기 전까지 추가하지 않는다.

---

# 61. 제품 실패 조건

다음 중 하나가 확인되면 현재 디자인을 고집하지 않는다.

### F1

State를 만들 이유가 없다.

→ Stage 0 실패.

### F2

실제 Physical Scale에서 Computation이 Infrastructure를 대체하는 Crossover가 존재하지 않는다.

→ Stage 1 실패.

### F3

Network Capacity가 단순 불편함만 만들고 Circuit 설계를 유도하지 않는다.

→ Capacity 설계 실패.

### F4

Capacity 최소화가 다른 모든 최적화 축을 압도한다.

→ Capacity 경제 실패.

### F5

Relay가 단순 Map Unlock Token에 불과하다.

→ Stage 2 실패.

### F6

전체 MVP가 작동하지만 플레이어가 회로를 다시 설계하고 싶어하지 않는다.

→ 제품 가설 실패.

---

# 62. GO Decision

현재 제품 설계에서 Stage 0 착수를 막는 미결 사항은 없다.

다음 결정은 확정한다.

```text
Computation Primitive
= AND / OR / NOT

World Medium
= Wire

Network Scale Constraint
= Global Network Capacity

Capacity Source
= Main Core + Captured Relay

Wire Cost
= Actual Physical Length

Capacity Behavior
= Soft Support Limit

Relay
= Fixed World Resource

Progression
= Physical Replication
  → Crossover
  → Computation Efficiency

Meta Progression
= Module Library
```

검증 순서는:

```text
Stage 0
Emergence

   ↓ PASS

Stage 1
Capacity Economy

   ↓ PASS

Stage 2
Relay Expansion

   ↓ PASS

MVP
Emergent Defense

   ↓ PASS

Free Web Alpha
External Validation
```

각 단계는 다음 단계의 가장 위험한 가정을 가장 싼 비용으로 먼저 검증한다.

---

# 63. 최종 판타지

처음에는 Wire가 답이다.

```text
문제 발생
→ Wire를 더 깐다
```

그리고 실제로 그것이 잘 작동한다.

하지만 문명이 커진다.

Wire가 길어진다.

Capacity가 부족해진다.

전력이 새고, Delay가 커지고, 건설과 수리가 늦어진다.

플레이어는 Relay를 확보한다.

문명은 다시 확장된다.

그러나 또 한계에 부딪힌다.

그때 플레이어가 처음으로 묻는다.

> **“선을 하나 더 까는 대신, 지금 있는 선에서 더 많은 일을 할 수 없을까?”**

Sensor를 더 까는 대신 과거를 기억한다.

전용선을 더 까는 대신 Signal을 Encoding한다.

목적지마다 Track을 까는 대신 Shared Network와 Route State를 만든다.

모든 방어선을 켜는 대신 필요한 Sector만 활성화한다.

더 많은 Radiation Wire를 만드는 대신 Energy의 Arrival Timing을 계산한다.

긴 Signal을 그대로 운반하는 대신 현장에서 정보를 처리한다.

결국 후반 문명의 강함은 Wire의 양으로 결정되지 않는다.

> **한 줄의 Wire에서 얼마나 많은 정보와 Energy와 행동을 끌어낼 수 있는가**

로 결정된다.

지역에는 작은 Hardwired Controller가 있다.

그 위에는 FSM이 있다.

Memory가 있다.

Programmable Computer가 있다.

Mobile Circuit이 Network를 돌아다니며 Infrastructure를 복구한다.

수천 개의 회로가 하나의 거대한 자동화 문명을 이룬다.

그러나 확대해보면 모든 계산은 여전히:

```text
AND
OR
NOT
```

뿐이다.

그리고 언젠가 적의 압력이:

* Power
* Network Capacity
* Construction
* Repair
* Timing
* Computation

모두를 넘어선다.

Main Core가 파괴된다.

문명은 사라진다.

다음 Run에 남는 것은 하나뿐이다.

> **이전보다 더 적은 Wire로 더 많은 일을 하도록 만든 설계.**

---

# AND OR NOT

> **문명은 무너진다. 설계는 남는다.**

**PRODUCT DECISION: GO**
