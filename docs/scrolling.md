# Scrolling

Both wheels work normally without any configuration. These settings change
them, per profile, like buttons:

```toml
[profile.default.scroll]
smooth = true        # hi-res smooth scrolling through the daemon
speed = 1.5          # 1 is the mouse's own speed

[profile.default.thumb-wheel]
mode = "volume"      # hscroll, volume, zoom, tabs or none
speed = 1.0
tap = "key:playpause"
```

## Main wheel

With `smooth = true` the daemon takes over the wheel, switches it to its
hi-res mode and sends hi-res scroll events, so pages move in small steps
rather than whole notches. Programs that only understand whole notches still
get one per notch. Over a Logi Bolt receiver this is the only way to get
hi-res scrolling on Linux today, since the kernel does not drive the
receiver. `speed` scales the distance.

Ratchet or free spin, SmartShift and torque are device settings and work the
same either way; see `familiar set`.

## Thumb wheel

Setting any `thumb-wheel` option takes over the thumb wheel:

| `mode` | Each notch |
|---|---|
| `hscroll` | Scrolls sideways, as normal, with `speed` (the default) |
| `volume` | Volume up or down |
| `zoom` | Control plus a wheel notch, which zooms in most apps |
| `tabs` | Next or previous tab with Control+Tab and Control+Shift+Tab |
| `none` | Nothing |

`tap` runs an action when you touch the thumb wheel briefly without turning it.

## Direction

Direction is set in one place only, the device settings `wheel-invert` and
`thumb-invert`. The desktop's natural scrolling setting applies on top, as it
does for any mouse.
