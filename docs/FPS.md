# FPS limits

Director sets the global frame rate limit in the NVIDIA driver or in RivaTuner Statistics Server (RTSS). It talks to both directly, so no extra tools are needed, and Director doesn't need to stay open afterwards. RTSS must be installed and running to use its limit.

## Use it

On **Now** or in a scene's **FPS limit** step:

1. Turn on **Change FPS limit**. With it off, Director leaves all FPS settings as they are.
2. Choose the method: NVIDIA or RTSS. Only methods available on this PC are listed; open RTSS first, then use **Check available methods**.
3. Choose **Set a frame rate limit** with a number or **VRR-safe cap**, or **Remove the global limit**.

On Now, the change waits for **Apply changes** on the display map. In a scene, it runs after the Sound step and before the After actions. A scene can contain only an FPS step. If a scene's method isn't available when it runs, the scene fails before changing anything; Director never switches to the other method.

```text
director fps status
director fps set nvidia 117
director fps set rtss vrr
director fps set nvidia off
```

## VRR-safe cap

**VRR-safe cap** limits frames a little below the primary display's refresh rate, so a variable refresh rate display stays in its VRR range. Director uses the curve NVIDIA Reflex uses, refresh − refresh²/3600, rounded down:

| Refresh rate | Cap |
| --- | --- |
| 240 Hz | 224 FPS |
| 165 Hz | 157 FPS |
| 144 Hz | 138 FPS |
| 120 Hz | 116 FPS |
| 60 Hz | 59 FPS |

The refresh rate is read when the cap is applied. In a scene that's after its Displays step, so switching to a 120 Hz TV gives 116. If the refresh rate can't be read, the step fails before changing anything. The step's dialog previews the value from the scene's display preset or your current layout.

## What the limit affects

- **NVIDIA** sets the frame rate limit in the driver's global profile, for every GPU and display. Game-specific profiles override it.
- **RTSS** sets its global profile's limit. Per-game RTSS profiles override it.
- Setting one method leaves the other alone. In-game limits, V-Sync and frame generation also affect the frame rate you see. A running game may need a restart to pick up a driver change.
- Limits: NVIDIA 1–1023 FPS; RTSS 1–1000 FPS. RTSS fractional and frame-time limits are left untouched.

Director reads the setting back after writing it, which confirms the configuration, not the frame rate a game reaches. If a later scene step fails, the previous limit is restored.

## How it works

NVIDIA uses the driver settings API (`nvapi64.dll` from System32) to change the global profile's frame rate limiter. RTSS is found through its installation registry entry and controlled through `RTSSHooks64.dll` from that folder; DLLs load only from the install folder and System32. Removing the RTSS limit writes 0 without disabling per-game limits. If RTSS runs elevated and Director doesn't, RTSS may refuse the change; Director reports that instead of elevating.

Per-game limits and AMD or Intel drivers aren't supported.

References: [NVIDIA driver settings API](https://docs.nvidia.com/nvapi/group__drsapi.html), [NVAPI headers](https://github.com/NVIDIA/nvapi), [rtss-cli](https://github.com/xanderfrangos/rtss-cli) (protocol reference; not bundled or run).
