# Integration references

Projects to consult for design and protocol research. Listing a project does not make it a Director dependency. The first LG setup/pairing slice is implemented; physical TV compatibility is awaiting validation (see TVS.md). Review the relevant source, API limitations, and license before reusing code.

| Project | Useful research | Director scope |
| --- | --- | --- |
| [DisplayProfileManager](https://github.com/exytral/DisplayProfileManager) | Per-monitor editor controls and display-profile workflows | Inspiration for the planned native monitor editor; preserve Director's independent presets and scenes |
| [DisplayHotKeys](https://github.com/jon-mil-92/DisplayHotKeys) | Display mode, refresh-rate, scaling, and hot-plug workflows | Display capability and usability research |
| [lgtvcli](https://github.com/01dnot/lgtvcli) | LG webOS discovery, pairing, Wake-on-LAN, power, input selection, and remote commands | Research for the planned direct LG integration; do not require this Python CLI |
| [samsung-tv-control](https://github.com/timdevlet/samsung-tv-control) | Local Samsung discovery/pairing, Wake-on-LAN, and remote WebSocket commands | Protocol reference for the implemented native Samsung preview; no Node/CLI dependency, no sequences or SmartThings |

Samsung's optional SmartThings cloud integration is not an accepted Director feature. Local protocol capabilities, input selection, and state reporting need device-specific validation before making support claims.

References reviewed on 2026-10-31. These are implementation references, not endorsements or compatibility guarantees.


## Samsung direct-command policy — 2026-11-03

The user explicitly rejected custom remote-key sequences and menu navigation fallbacks. The implemented Samsung preview offers only tested direct commands; hide unsupported controls rather than substituting a sequence. SmartThings remains optional/unapproved research.

ColorControl's SamsungDevice.cs uses the local samsung.remote.control WebSocket, Wake-on-LAN, individual ms.remote.control key commands and preset sequences. It does not supply a universal confirmed HDMI-port selection API. Its built-in SamsungButtons list contains KEY_HDMI and KEY_SOURCE, not numbered HDMI1–4 entries. Direct numbered HDMI keys still require model-specific testing. An individual discrete HDMI command is distinct from a menu-navigation sequence; no sequence fallback is accepted.

Its GetPowerStateAsync reads device.PowerState from http://<tv>:8001/api/v2/ when provided. This is more informative than treating every HTTP response as On; transport failure must still remain Unknown in Director. Its PowerOffAsync sends KEY_POWER, tolerates a timeout and then marks local state off; that does not independently verify physical shutdown.

Sources: https://github.com/Maassoft/ColorControl/blob/master/ColorControl/Services/Samsung/SamsungDevice.cs and https://github.com/Maassoft/ColorControl/blob/master/Shared/Contracts/Samsung/SamsungButtons.cs. This source review adds no Samsung implementation or compatibility guarantee.

Samsung local pairing/control and explicit compatibility testing are now implemented; SAMSUNG.md documents current behavior and physical-validation limits. This supersedes the earlier deferred implementation status above.

## TCL Android / Google TV reference

[tronikos/androidtvremote2](https://github.com/tronikos/androidtvremote2) and the Google Polo protocol are references for the local code-pairing and Remote v2 wire subset. Director implements control in Rust; neither Python nor an external TV CLI is a runtime dependency. See ANDROID_TV.md for scope and physical testing limitations.

## NVIDIA and RTSS FPS references

NVIDIA’s official NVAPI DRS headers and interface table define the FRL_FPS setting and ABI. xanderfrangos/rtss-cli and the installed RTSS SDK identify the profile/flag interfaces. Director implements independent Rust native calls and does not redistribute the driver/RTSS or invoke either CLI. See FPS.md for supported global scope and validation limitations.
