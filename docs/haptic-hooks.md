# Haptic hooks

Any program can make the mouse buzz with `familiar haptic <waveform>` or the
D-Bus method `PlayHaptic` (see `docs/dbus.md`). List the waveforms your mouse
supports with `familiar haptic`.

These snippets buzz when a command that ran for a while finishes:
`completed` when it succeeded, `angry-alert` when it failed. Set
`FAMILIAR_LONG_COMMAND` to the number of seconds that counts as long; the
default is 10.

## bash

Add to `~/.bashrc`. Needs bash 5 for `EPOCHSECONDS`.

```bash
__familiar_ready=1
__familiar_preexec() {
    if [ "$__familiar_ready" = 1 ]; then
        __familiar_start=$EPOCHSECONDS
        __familiar_ready=0
    fi
}
__familiar_status() { __familiar_last=$?; }
__familiar_precmd() {
    if [ -n "${__familiar_start:-}" ] &&
        [ $((EPOCHSECONDS - __familiar_start)) -ge "${FAMILIAR_LONG_COMMAND:-10}" ]; then
        if [ "$__familiar_last" = 0 ]; then w=completed; else w=angry-alert; fi
        (familiar haptic "$w" >/dev/null 2>&1 &)
    fi
    unset __familiar_start
    __familiar_ready=1
}
trap '__familiar_preexec' DEBUG
PROMPT_COMMAND="__familiar_status${PROMPT_COMMAND:+;$PROMPT_COMMAND};__familiar_precmd"
```

`__familiar_status` runs first so it sees the command's exit status, and
`__familiar_precmd` runs last so other prompt commands are not timed.

## zsh

Add to `~/.zshrc`.

```zsh
zmodload zsh/datetime
autoload -Uz add-zsh-hook
__familiar_preexec() { __familiar_start=$EPOCHSECONDS }
__familiar_precmd() {
    local s=$?
    [[ -n $__familiar_start ]] || return
    local took=$(( EPOCHSECONDS - __familiar_start ))
    unset __familiar_start
    if (( took >= ${FAMILIAR_LONG_COMMAND:-10} )); then
        if (( s == 0 )); then w=completed; else w=angry-alert; fi
        familiar haptic $w &>/dev/null &!
    fi
}
add-zsh-hook preexec __familiar_preexec
add-zsh-hook precmd __familiar_precmd
```

## fish

Save as `~/.config/fish/conf.d/familiar.fish`.

```fish
function __familiar_postexec --on-event fish_postexec
    set -l s $status
    set -q FAMILIAR_LONG_COMMAND; or set -l FAMILIAR_LONG_COMMAND 10
    if test $CMD_DURATION -ge (math $FAMILIAR_LONG_COMMAND \* 1000)
        if test $s -eq 0
            familiar haptic completed >/dev/null 2>&1 &
        else
            familiar haptic angry-alert >/dev/null 2>&1 &
        end
    end
end
```

## systemd

`packaging/systemd/familiar-haptic@.service` plays the waveform named after the
`@`. Point a user unit's `OnFailure=` at it to feel when that unit fails:

```ini
[Unit]
OnFailure=familiar-haptic@angry-alert.service
```

It works for user units only, since system services cannot reach your session
bus. `packaging/install-user.sh` installs it.

## One-off

```sh
make && familiar haptic completed || familiar haptic angry-alert
```
