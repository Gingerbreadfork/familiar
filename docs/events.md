# Event stream

`familiar events --json` prints one JSON object per line for every event
the mouse sends. Add `--divert <control>`, `--raw-xy`, `--wheel` or `--thumb`
to route those controls to the stream instead of the desktop; they are put
back when the command exits.

Every line has these fields:

| Field | Type | |
|---|---|---|
| `v` | number | Schema version, currently 1. Fields are only added within a version |
| `time_ms` | number | Unix time in milliseconds when the event was read |
| `device` | number | Receiver slot, or 255 for a direct connection |
| `feature` | string or null | HID++ feature ID such as `"1B04"`; null for receiver notifications |
| `event` | number | Event number within the feature |
| `kind` | string | One of the kinds below |
| `raw` | string | The whole report in hex |

| `kind` | Extra fields |
|---|---|
| `buttons` | `held`: names of diverted controls held now, such as `["gesture"]`; an empty list means all released |
| `raw-xy` | `dx`, `dy`: pointer movement while a control diverted with `--raw-xy` is held |
| `wheel` | `delta`, `periods`, `hires` |
| `ratchet` | `ratchet`: true when the wheel switched to ratchet mode |
| `thumb-wheel` | `delta`, `rotation` (`start`, `active`, `stop`, `inactive`), `touch`, `tap` |
| `battery` | `percent` (null when unknown), `charging` |
| `connection` | `linked`, `wpid` |
| `other` | none |

Control names: `left`, `right`, `middle`, `back`, `forward`, `gesture`,
`smartshift`, `haptic` (the thumb panel), `virtual-gesture`, or a hex control
ID for anything else.

Example:

```sh
familiar events --json --thumb | jq -c 'select(.kind == "thumb-wheel") | .delta'
```
