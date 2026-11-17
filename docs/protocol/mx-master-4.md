# MX Master 4 over HID++

Observed on one unit, firmware RBM 27.00.B0015, connected through a Logi Bolt
receiver (`046d:c548`) at device index 2. Produced with `familiar dump` and
`familiar dump --research`.

## Transport

- Bolt receiver: the HID++ interface is the hidraw node whose report
  descriptor declares report IDs `0x10` and `0x11` under usage page `0xFF00`.
  There is no very long (`0x12`) report.
- The kernel binds `hid-generic` to the receiver, not `hid-logitech-dj`, so the
  wheel stays in low-res HID mode unless we change it.
- Model ID `B04200000000`, transport `btleid B042`. Protocol 4.5.

## Reply framing

Normal reply: `id dev feature_index function|swid params...`

Error reply: `id dev 0xFF feature_index function|swid code ...` for HID++ 2.0,
and the same with `0x8F` for HID++ 1.0. The original feature index moves into
the address byte, so the error code is the second parameter byte.

## Feature table

| Index | ID | Version | Flags | Notes |
|---|---|---|---|---|
| 0 | `0000` Root | 0 | | |
| 1 | `0001` Feature Set | 2 | | |
| 2 | `0003` FW Version | 8 | | 4 entities: LD 04.00, RBM 27.00.B0015 twice, hardware 72 |
| 3 | `0005` Name | 5 | | "MX Master 4", kind mouse |
| 4 | `1D4B` Wireless Status | 0 | | |
| 5 | `0020` Config Change | 1 | | |
| 6 | `0021` Crypto ID | 1 | | |
| 7 | `0007` Friendly Name | 0 | | "MX Master 4" |
| 8 | `0011` Property Access | 0 | | |
| 9 | `1004` Unified Battery | 5 | | |
| 10 | `1701` unknown | 0 | | function 0 returns `01 00 0a 00...` |
| 11 | `19B0` Haptic | 0 | | |
| 12 | `19C0` Force Sensing Button | 0 | | |
| 13 | `1B04` Reprog Controls v4 | 6 | | |
| 14 | `1814` Change Host | 2 | | |
| 15 | `1815` Hosts Info | 2 | | |
| 16 | `2250` XY Stats | 1 | | |
| 17 | `2111` SmartShift Enhanced | 0 | | |
| 18 | `2121` HiRes Wheel | 1 | | |
| 19 | `2150` Thumb Wheel | 0 | | |
| 20 | `2201` Adjustable DPI | 2 | | |
| 21 | `2251` Wheel Stats | 0 | | |
| 22 | `00D1` firmware update | 0 | | never called |
| 23-34, 36-45 | `18xx`, `1Exx`, `9xxx` | | hidden, internal | never called |
| 35 | `1602` unknown | 0 | | function 0 with no arguments returns invalid argument |

## Settings

| Feature | Function | Layout | Observed |
|---|---|---|---|
| `2201` DPI | 1 `getSensorDpiList(sensor, 0, page)` | `sensor, list...`; a value `0xE000 | step` fills the range to the next value | 200 to 8000 in steps of 50 |
| | 2 `getSensorDpi(sensor)` | `sensor, current:u16, default:u16` | 1000 / 1000 |
| | 3 `setSensorDpi(sensor, dpi:u16)` | | verified |
| `2111` SmartShift | 0 `getCapabilities` | `flags (bit0 tunable torque), default threshold, default torque` | `01 0a 4b` |
| | 1 `get` | `mode (1 free, 2 ratchet), auto-disengage (0xFF never), torque` | ratchet, 10, 75 |
| | 2 `set` | same; mode must be sent for torque and threshold to apply | verified |
| `2121` HiRes Wheel | 0 caps | `multiplier, flags (0x08 invert, 0x04 ratchet switch)` | 15, both |
| | 1/2 get/set mode | bit0 diverted, bit1 hi-res, bit2 inverted | all off |
| | 3 ratchet state | bit0 ratchet | |
| `2150` Thumb Wheel | 0 info | `native res:u16, diverted res:u16, default dir, caps, time unit:u16` | 20, 120, touch and timestamp, 1000 |
| | 1/2 status/set | `diverted, inverted` | verified |
| `19B0` Haptic | 0 caps | bytes 4..8 waveform bitmask (big endian) | raw `00 01 00 3c 00 00 7f ff`: waveforms 0 to 14 |
| | 1 get level | `bit0 enabled, percent, bit0 stepped` | enabled, 100, continuous |
| | 2 set level | `enabled, percent` | verified, 0 sends `00 <previous>` |
| | 4 play | `waveform` | verified |
| `19C0` Force Button | 0 count | | 1 |
| | 1 info | `flags:u16 (bit0 changeable), default:u16, max:u16, min:u16` | 4630, 6158, 3704 |
| | 2 get / 3 set | `current:u16` / `number, value:u16` | verified |
| `1814` Change Host | 0 info | `count, current` | 3, 0 |
| `1815` Hosts Info | 0 / 1 / 3 | info, host info, name chunks of 14 bytes | host 1 "desktop" |

Unknown in the haptic capabilities: bytes 1 (`01`) and 3 (`3c`, 60). Byte 3 may
be a maximum duration or a level count. To investigate.

## Controls

| CID | Name | Task | Capabilities |
|---|---|---|---|
| `0050` | left | `0038` | mouse, analytics |
| `0051` | right | `0039` | mouse, analytics |
| `0052` | middle | `003A` | reprogrammable, divertable, raw-xy |
| `0053` | back | `003C` | reprogrammable, divertable, raw-xy |
| `0056` | forward | `003E` | reprogrammable, divertable, raw-xy |
| `00C3` | gesture | `009C` | reprogrammable, divertable, raw-xy |
| `00C4` | smartshift | `009D` | reprogrammable, divertable, raw-xy |
| `01A0` | haptic panel | `0109` | reprogrammable, divertable, raw-xy |
| `00D7` | virtual gesture | `00B4` | divertable, virtual, raw-xy, force-raw-xy |

Nothing is persistently divertable, so diversion is always temporary and is
lost when the mouse sleeps or reconnects. The daemon must reapply it.

## Events

Thumb wheel (`2150` event 0), captured: `00 02 00 00 02 02` is delta +2,
timestamp 0, rotation active, flags `0x02` (touch). The stop event is
`00 00 00 00 03 00`. Event flag bits follow the capability bits: `0x02`
touch, `0x04` proximity, `0x08` single tap.

## Diverted controls and pointer movement

Captured with `familiar events --json` while pressing each control.

| Control | Physical button | Plain HID when not diverted |
|---|---|---|
| `0052` middle | Pressing the wheel | `BTN_MIDDLE` |
| `0053` back | Lower of the two side buttons | `BTN_SIDE` |
| `0056` forward | Upper of the two side buttons | `BTN_EXTRA` |
| `00C4` smartshift | Button behind the wheel | handled by the mouse |
| `01A0` haptic | The thumb panel | `BTN_BACK` (button 7) |
| `00C3` gesture | Front side button, ahead of forward and back (from product views; not yet captured) | |

All plain button reports arrive on the receiver's mouse input device. The
receiver's `hid-multitouch` interface sends nothing when the thumb panel is
pressed.

Diverted presses arrive as `0x1B04` event 0 with up to four held control IDs,
and release as an empty list. With raw XY, event 1 carries `dx` and `dy` as
big-endian signed 16-bit values while the control is held: about 78 events a
second when moving. **The first event 1 after each press is not a movement
delta** (seen: `-6125,-7389`, `-1341,-583`, `-8,-1`) and must be ignored. The
thumb panel supports raw XY, which is what the Actions Ring needs.

## Wheel events

`0x2121` event 0 is `flags, delta:i16`; flags bits 0-3 are periods and bit 4
is hi-res. Diverted in low-res mode each notch is one event with delta ±1.
In hi-res mode deltas of up to ±25 arrive per event, covering one or two
periods, as little as 6 ms apart; one notch is 15 counts.

## Thumb wheel events

`0x2150` event 0, captured while rolling, resting on and tapping the wheel:

- `delta:i16`, then `elapsed:u16`, the milliseconds since the previous
  rotation event (0 on the first), then the rotation phase, then flags.
- Phases run start (1), active (2), stop (3), then inactive (0).
- Flag `0x02` is touch: set as soon as the thumb lands on the wheel, cleared
  when it lifts, with or without rotation.
- Slow rolls send ±1 per event, quick ones up to about 13, as little as 7 ms
  apart. Diverted resolution is 120 counts per turn.
- There is no tap flag on this mouse, as `getInfo` says. A tap shows as a
  short touch with no rotation, so taps can be detected in software.

## Haptic timing

`0x19B0` function 4 accepts a new waveform every 12 to 25 ms with no busy
error, even with no gap between requests. Three knocks 150 ms and 50 ms apart,
and three sharp clicks with no gap, were all felt; whether the zero-gap clicks
felt like three separate taps was not confirmed.

## Host names

`0x1815` function 4 is `setHostFriendlyName(host, offset, up to 14 bytes)`.
Capability bit `0x02` of function 0 says names can be written; byte 5 of
function 1 is the maximum length. The device sets the stored length from the
last chunk written, so a shorter name needs no padding. Verified by writing an
18-byte name in two chunks, then a 6-byte one.

## Open questions

- Feature `1701`: function 0 returns `01 00 0a`. Possibly a capabilities
  reply; meaning unknown.
- Feature `1602`: likely BLE password authentication; needs arguments.
- Whether Options+ uses `00D7` with force-raw-xy for the Actions Ring. Not
  tried yet: forced raw XY freezes the pointer while active.
- Why the first raw XY event after a press is not a delta.
- Layouts of `2250` and `2251` statistics.
- Haptic capability bytes 1 and 3, and whether functions 3, 5 and up exist.
