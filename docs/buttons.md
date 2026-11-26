# Buttons and gestures

Buttons are set in `[profile.default.buttons]` in the config file. The daemon
applies changes as soon as the file is saved.

```toml
[profile.default.buttons]
back = "key:alt+left"
forward = "key:alt+right"
middle = "mouse:middle"
smartshift = "smartshift:toggle"
haptic = { tap = "key:super", hold = "haptic:completed", up = "key:volumeup", down = "key:volumedown" }
```

## Buttons

| Name | Button |
|---|---|
| `middle` | Pressing the wheel |
| `back` | Lower of the two side buttons |
| `forward` | Upper of the two side buttons |
| `smartshift` | The button behind the wheel |
| `haptic` | The thumb panel |
| `gesture` | The side button nearest the front, ahead of forward and back |

Left and right click cannot be changed. A button you do not list keeps its
normal behaviour.

## Actions

| Action | Does |
|---|---|
| `key:ctrl+shift+t` | A keyboard shortcut that follows the button: Ctrl, Shift, Alt and Super stay down while you hold it and the other keys are pressed once, so `key:alt+tab` keeps the window switcher open until you let go. A key on its own is held. Names: letters and digits, `f1` to `f24`, `ctrl`, `shift`, `alt`, `super`, `enter`, `esc`, `tab`, `space`, `up`, `pageup`, `home`, `volumeup`, `mute`, `playpause`, `next`, `brightnessup`, `print` and more, or any evdev name such as `KEY_ZOOM` |
| `press:alt+tab` | The same shortcut pressed and let go at once, however long you hold the button |
| `mouse:left` | A mouse button: `left`, `right`, `middle`, `back`, `forward` |
| `shell:command` | Runs the command with `sh -c`; `FAMILIAR_CONTROL` and `FAMILIAR_GESTURE` say what triggered it. Under the systemd service it is started with `systemd-run` as a unit of its own, so it can use the network, which the daemon cannot, and it keeps running when the daemon restarts |
| `haptic:completed` | Plays a waveform; `familiar haptic` lists them |
| `dpi:1600` | Sets the DPI |
| `dpi-cycle:800,1600,3200` | Steps through DPI values |
| `dpi-shift:400` | Uses this DPI while the button is held, for precise work |
| `smartshift:toggle` | Switches the wheel between ratchet and free spin |
| `host:2` | Easy-Switch to channel 2 |
| `none` | Does nothing |

A button bound straight to an action follows the button: `key:` shortcuts and
mouse buttons stay pressed while you hold it. A button bound to another mouse
button, such as `back = "mouse:middle"`, is remapped inside the mouse, so the
mouse sends that button itself and nothing is injected. Like every binding it
lasts while the daemon runs; stopping the daemon puts the button back.

## Gestures

Give a table instead of a single action:

| Key | When |
|---|---|
| `tap` | Pressed and released |
| `double` | Tapped twice quickly. Only when this is set does a single tap wait a moment to see whether a second follows |
| `hold` | Held for half a second without moving; the action lasts until release |
| `up`, `down`, `left`, `right` | Held while moving the mouse that way, then released |
| `up-left`, `up-right`, `down-left`, `down-right` | Setting any of these switches to eight directions |
| `toggle` | Press once to hold the action down, press again to let go; for anyone who finds holding a button hard. Cannot be combined with the others |

The pointer keeps moving while you swipe; only the direction is used.

## While the screen is locked

Nothing is sent while the session is locked, and anything held is let go.

## When something does not work

`familiar status` lists bindings the mouse could not take under
"not applied", for example a button the mouse does not have.
`familiar undivert` puts every button back to normal at any time.
