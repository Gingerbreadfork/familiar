# D-Bus API

`familiard` owns `io.github.gingerbreadfork.Familiar` on the session bus.

## `io.github.gingerbreadfork.Familiar.Manager1` at `/io/github/gingerbreadfork/Familiar`

| Member | Type | |
|---|---|---|
| `Devices` | property `ao` | Every device seen since the daemon started |
| `Conflicts` | property `as` | Other running tools that configure Logitech devices |
| `ConfigPath` | property `s` | |
| `Version` | property `s` | |
| `Reload()` | method | Reread the config and apply it; returns once applied |

## `io.github.gingerbreadfork.Familiar.Device1` at `/io/github/gingerbreadfork/Familiar/devices/d<unit-id>`

| Member | Type | |
|---|---|---|
| `Name`, `UnitId`, `Model` | property `s` | `Model` is the product ID, e.g. `B042` |
| `Endpoint` | property `s` | hidraw node, e.g. `/dev/hidraw6` |
| `Index` | property `y` | Receiver slot, or 255 when connected directly |
| `Online` | property `b` | False while asleep or on another host |
| `BatteryPercent` | property `y` | 0 when unknown |
| `BatteryStatus` | property `s` | `discharging`, `recharging`, `almost-full`, `full`, `slow-recharge`, `error`, `unknown` |
| `Settings` | property `a{ss}` | Current values, same names and forms as `familiar set` |
| `ApplyErrors` | property `as` | Settings and button bindings that could not be put on the device |
| `Configured` | property `a{ss}` | What the config file asks for this device, same form as `Settings` |
| `LastApplied` | property `t` | Unix time settings were last applied; 0 before the first time |
| `ReconnectMs` | property `u` | Milliseconds from the last reconnect notice to settings being in place; 0 if none yet |
| `Set(name s, value s, persist b) → a{ss}` | method | Apply one setting; with `persist`, save it to the config |
| `PlayHaptic(waveform s)` | method | e.g. `completed`, `happy-alert` |
| `SwitchHost(channel y)` | method | Easy-Switch channel 1-3 |
| `Hosts() → current y, hosts a(ybs)` | method | The current channel, and each channel as (channel, paired, name); channels count from 1 |
| `RenameHost(channel y, name s) → s` | method | Renames a channel; returns the name as the mouse stored it, cut to its maximum length |
| `Reapply()` | method | Apply the configured settings again |

All properties emit `PropertiesChanged`. Methods fail with
`org.freedesktop.DBus.Error.Failed` and a readable message.

## Examples

```sh
busctl --user get-property io.github.gingerbreadfork.Familiar /io/github/gingerbreadfork/Familiar/devices/d1A2B3C4D \
    io.github.gingerbreadfork.Familiar.Device1 BatteryPercent

busctl --user call io.github.gingerbreadfork.Familiar /io/github/gingerbreadfork/Familiar/devices/d1A2B3C4D \
    io.github.gingerbreadfork.Familiar.Device1 PlayHaptic s completed

# Buzz the mouse when a long command finishes.
make && busctl --user call io.github.gingerbreadfork.Familiar /io/github/gingerbreadfork/Familiar/devices/d1A2B3C4D \
    io.github.gingerbreadfork.Familiar.Device1 PlayHaptic s completed
```
