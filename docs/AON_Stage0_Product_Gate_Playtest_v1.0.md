# A/O/N Stage 0 Product Gate A/B Playtest

**Environment:** Windows native / PowerShell
**Purpose:** 동일한 현재 입력을 받는 두 설계를 직접 번갈아 플레이하여 Stage 0 Emergence 가설을 PASS 또는 FAIL로 판정한다.

자동 테스트와 Replay golden은 기술 정합성을 증명하지만 제품 판정을 대신하지 않는다. 이 플레이테스트는 과거 입력으로 생긴 내부 State가 이후 World 행동을 실제로 바꾸는지, 그리고 그 차이가 사용자에게 읽히는지를 확인한다.

## 실행

저장소 루트의 Windows PowerShell에서 실행한다. WSL은 사용하지 않는다.

```powershell
cargo run -p aon-app --locked --offline -- stage0-product-probe
```

앱은 기본적으로 `F6 RETAINED STATE` 설계의 `nextTick=24` 준비 지점에서 PAUSED 상태로 열린다. 두 설계 모두 Mobile 4가 미리 선택되어 있고 다음 네 waveform probe가 연결되어 있다.

- `driver:7` — SET 입력
- `gate:6:out` — Q
- `gate:8:out` — Qbar
- Mobile 4 STOP Sink

준비 상태는 두 설계 모두 `SET=0`, `Q=0`, `Qbar=1`, `STOP=0`이다.

## A/B 전환과 조작

- `F5`: **CURRENT INPUT ONLY** 설계를 새 세션으로 열고 `nextTick=24`로 재설정
- `F6`: **RETAINED STATE** 설계를 새 세션으로 열고 `nextTick=24`로 재설정
- `Space`: 재생/일시정지
- `.`: 한 Tick 진행
- `1`, `2`, `3`: 1/4x, 1x, 4x
- `C`: 선택된 Mobile 4의 Circuit View
- `N`: Network View
- `R`: 현재 Replay의 원시 Tick 0으로 재시작

`F5` 또는 `F6`은 실행 중 어느 시점에서 눌러도 해당 설계의 준비 지점으로 돌아가며 기본 probe와 Mobile 선택도 복원한다. 공정한 비교를 위해 `R`보다 `F5`/`F6`을 사용한다. 정밀 관찰에는 먼저 `1`을 눌러 1/4x로 바꾼 뒤 `Space`를 누르는 것이 좋다.

## 동일 입력 타임라인

UI에는 다음에 실행될 Tick인 `nextTick`이 표시된다. 두 Replay의 SET 명령 타임라인은 완전히 같다.

| nextTick | CURRENT INPUT ONLY (`F5`) | RETAINED STATE (`F6`) |
|---:|---|---|
| 24 | SET=0, Q=0, Qbar=1, STOP=0 | SET=0, Q=0, Qbar=1, STOP=0 |
| 70 | SET pulse 직전 | SET pulse 직전 |
| 71 | SET=1, Q=0, Qbar=1, STOP=0 | SET=1, Q=0, Qbar=1, STOP=0 |
| 81 | SET=1, Q=1, Qbar=0, STOP=1; Mobile 정지 | SET=1, Q=1, Qbar=0, STOP=1; Mobile 정지 |
| 97 | SET release 직전 | SET release 직전 |
| 98 | SET=0, Q=1, Qbar=0, STOP=1 | SET=0, Q=1, Qbar=0, STOP=1 |
| 162 | SET=0, Q=0, Qbar=1, STOP=0; Mobile 재이동 | SET=0, Q=1, Qbar=0, STOP=1; Mobile 정지 유지 |

두 Replay는 `nextTick=162`에 자동으로 끝나고 PAUSED 상태가 된다.

## 권장 비교 순서

1. `F5`를 눌러 CURRENT INPUT ONLY를 `nextTick=24`에서 시작한다.
2. `Space`로 `nextTick=162`까지 재생한다. SET이 다시 0이 된 뒤 Q와 STOP이 0으로 돌아가고 Mobile이 재이동하는지 확인한다.
3. `F6`을 눌러 RETAINED STATE를 동일한 `nextTick=24`에서 시작한다.
4. 같은 방식으로 `nextTick=162`까지 재생한다. 같은 현재 입력 `SET=0`인데 Q와 STOP이 1로 유지되고 Mobile이 멈춰 있는지 확인한다.
5. 필요하면 `F5`와 `F6`을 다시 눌러 Network View, Circuit View, waveform, `A/B Emergence Inspector`를 비교한다.

핵심은 단순히 두 화면이 다른지가 아니다. 동일한 SET 타임라인과 동일한 pulse 직후 행동을 거친 뒤, 오직 retained feedback State의 유무 때문에 release 이후 World 행동이 갈라지는지를 판단한다.

## 판정 기록

아래 질문을 사용자가 직접 답한다.

```md
Environment: Windows native
Commit: <commit SHA 또는 working-tree>

- F5/F6의 SET 타임라인과 pulse 직후 행동이 동일하다는 점이 명확했는가: YES / NO
- 같은 현재 LOW 입력에서 retained State 유무만으로 최종 행동이 갈라졌는가: YES / NO
- 별도 Runtime Class가 아니라 회로의 feedback State가 행동을 만들었다는 점이 명확했는가: YES / NO
- State가 장식이 아니라 World 행동에 실제로 필요하다고 느꼈는가: YES / NO
- pulse → feedback → STOP 인과를 읽을 수 있었는가: YES / NO
- 이 회로를 디버깅하고 개선해 보고 싶은가: YES / NO

Stage 0 product verdict: PASS / FAIL
Rationale: <한두 문장>
```

여섯 질문이 모두 YES이고 사용자가 명시적으로 PASS를 기록하기 전에는 Stage 0 product gate를 완료로 표시하지 않으며 Stage 1 전체 구현을 시작하지 않는다.

## Recorded verdict

```md
Date: 2026-08-12
Environment: Windows native
Commit: working-tree immediately preceding the Stage 0 closure commit

- F5/F6의 SET 타임라인과 pulse 직후 행동이 동일하다는 점이 명확했는가: YES
- 같은 현재 LOW 입력에서 retained State 유무만으로 최종 행동이 갈라졌는가: YES
- 별도 Runtime Class가 아니라 회로의 feedback State가 행동을 만들었다는 점이 명확했는가: YES
- State가 장식이 아니라 World 행동에 실제로 필요하다고 느꼈는가: YES
- pulse → feedback → STOP 인과를 읽을 수 있었는가: YES
- 이 회로를 디버깅하고 개선해 보고 싶은가: YES

Stage 0 product verdict: PASS
Rationale: The user gave an explicit unqualified PASS after the matched F5/F6 direct-play A/B
probe and authorized the Stage 0 commit and continued milestone development.
```
