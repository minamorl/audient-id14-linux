# iD14 realtime SR for PipeWire

This crate exposes the embedded `id14-sr` model as a stereo LADSPA 1.1
plugin and provides a user command that installs it transparently in front of
both iD14 playback outputs with WirePlumber Smart Filters. It does not replace or detach
`snd-usb-audio`.

The plugin accepts arbitrary planar host block sizes. It buffers them into
256-frame interleaved model chunks without allocation, blocking, file I/O or
logging in the audio callback. The adapter contributes 256 frames (5.33 ms)
on top of the model's 768 frames (16 ms), for 21.33 ms fixed DSP/adapter
delay at 48 kHz. At unsupported sample rates it safely passes audio through.

## Full-band protection

Each 256-frame chunk is classified using its Hann-windowed spectrum. The
detector compares energy at 13.5–20 kHz with 0.75–12 kHz. Completion fades in
only for plausibly band-limited material (ratio at or below `0.0002`) and is
fully bypassed for full-band material (ratio at or above `0.002`). The
intermediate log-ratio is blended continuously. Fade-in is 0.125 per chunk;
full-band release is 0.5 per chunk. Silence is bypassed. This conservative
detector can still misclassify naturally dark or codec-limited recordings;
the phase-1 heldout metrics are objective spectral metrics, not a subjective
quality claim.

## Linux build and install

Build on the target Linux host so the shared object uses its ABI:

```sh
cargo build --release -p id14-sr-ladspa
crates/id14-sr-ladspa/linux/install-user.sh
```

Installation copies the command and plugin under `~/.local` plus a dedicated,
disabled user service under `~/.config/systemd/user`; it leaves the feature
off. The command uses `libpipewire-module-filter-chain` plus WirePlumber Smart
Filter metadata. Both existing and new native PipeWire or Pulse streams whose
actual target is either iD14 Line/Headphones sink are transparently
linked through the plugin, including streams with an explicit physical target.
It does not change the global default or write per-application targets, so
streams on HDMI, Bluetooth, VR, and null sinks retain their route throughout
switching.

The precise scope is ordinary PipeWire-native and Pulse-compat playback whose
default or explicit target is either iD14 UCM Line or Headphones. Applications
that open ALSA `hw:`/`plughw:` directly, raw six-channel or pro-audio profiles,
and JACK/direct PipeWire port connections bypass this user-space Smart Filter.
Covering those hardware-direct paths would require a different integration
boundary and is intentionally outside this user-space design.

The iD14 UCM Line and Headphones nodes are themselves filters sharing one raw
six-channel ALSA node. One private, silent loopback node per UCM output gives
each filter a unique Smart Filter target; audio still follows
`application -> SR -> matching UCM filter -> raw iD14`. This prevents Line and
Headphones from being conflated by the shared raw target. Line reaches raw
AUX2/3 and Headphones reaches raw AUX0/1.

```sh
id14-sr status
id14-sr on              # Line and Headphones (normal mode)
id14-sr on line         # compatibility: Line only
id14-sr on headphones   # compatibility: Headphones only
id14-sr off
```

The playback mix is adjustable while `on` without stopping the filter or
changing the Line/Headphones routes. The default `auto` uses the existing
conservative bandwidth detector. A manual integer percentage from 0 to 200
requests restoration strength: `0` reaches the engine's fixed-delay bypass
after a short ramp (and starts in bypass if set before `on`); `100` requests
normal restoration, and `101`–`200` requests a stronger effect on material
with missing high frequencies. The engine protects recorded high frequencies
even under a manual setting, so the audible effect can be much smaller on
full-band music. It preserves existing highband samples and suppresses new
highband energy when the source already contains it. At every active mix,
a shared stereo gain limits buffered peaks to full scale, including the
overlap tail after reducing mix. Steady `0` preserves its fixed-delay source
output. Changes ramp at 10 percentage points per 256-frame chunk. The
setting survives `off`, `on`, and service restarts. On upgrade, an absent mix
file or a numeric file from the earlier detector-scaled implementation stays
`auto` until a new manual value is set.

```sh
id14-sr mix get          # prints auto or one integer, 0..200
id14-sr mix set 35       # applies to every enabled iD14 output
id14-sr mix set 150      # stronger restoration when high frequencies are missing
id14-sr mix auto         # restores conservative bandwidth detection
id14-sr mix step +5      # clamps at 200; -5 clamps at 0
id14-sr mix bar          # Waybar JSON: text, tooltip, class, percentage, mode
id14-sr mix slider       # optional Zenity slider, applies while dragging
```

For UI clients, `mix get` is the literal `auto` or an integer line. `status`
includes `mix=auto` or `mix=N`. `mix bar` returns one JSON object with `mode`
(`auto` or `manual`), `class` (`on`, `off`, or `degraded`), and `percentage`
(`0` in auto mode, otherwise `N`), alongside `text` and `tooltip`. The displayed
percentage is the requested setting, not a measurement of effective restoration;
existing source high frequencies may reduce the audible effect.

For Hyprland Waybar, an example custom module is:

```json
"custom/id14-sr": {
  "exec": "id14-sr mix bar",
  "return-type": "json",
  "interval": 1,
  "on-click": "id14-sr mix slider",
  "on-scroll-up": "id14-sr mix step +5",
  "on-scroll-down": "id14-sr mix step -5"
}
```

`mix get` and `mix bar` work while processing is off. `mix set` while on
requires both selected SR nodes and updates each live PipeWire `Props.params`
control. The CLI saves the setting only after both updates succeed, restores
the previous value on a failed update, and serializes simultaneous slider and
scroll commands with `flock`. Live changes require `pw-cli`; `mix slider`
requires `zenity` and uses its `--print-partial` output so movement applies
before the dialog closes. In `auto`, a new slider starts at 0; moving it enters
manual mode. Cancel restores the mix from before the dialog opened. `mix step`
from `auto` also starts at 0. The control file is under
`${XDG_STATE_HOME:-~/.local/state}/id14-sr/mix`; audio callbacks never read it.
PipeWire's filter-chain control support and `pw-cli set-param` syntax are
documented in the [filter-chain manual](https://docs.pipewire.org/page_module_filter_chain.html)
and [pw-cli manual](https://docs.pipewire.org/page_man_pw-cli_1.html).

`on` and `off` are idempotent. `on` writes one marked PipeWire fragment and
enables/starts `id14-sr-filter.service`; its default `all` mode creates two
independent filter/transport pairs. `ExecStartPost` waits for every requested
UCM sink and reapplies the dynamic Smart Filter metadata after either the
filter service or WirePlumber is restarted. `status` lists active and degraded
outputs separately, and a repeated `on` repairs missing metadata. If any step
after the first managed write fails, ON stops/disables the service and removes
the managed nodes, configuration, state, and owned metadata before returning
failure.

`off` removes only the five keys marked by `id14-sr.owner`, preserving other
applications' metadata on the same nodes. It confirms each output node name
before deletion and never deletes through a saved numeric ID alone. If an iD14
output is absent, service/config cleanup proceeds and a small per-output
`metadata-pending-*` state is retained; run `id14-sr off` again after the
device returns (or after WirePlumber has recreated its metadata) to finish cleanup.
Calling `off` while already off with no pending cleanup changes neither the
default nor active streams. A fresh installation starts disabled and
unprocessed.

## Recovery

If audio is silent, run:

```sh
id14-sr off
wpctl status
```

If the command itself is missing, stop the owned service before inspecting the
graph:

```sh
systemctl --user disable --now id14-sr-filter.service
wpctl status
```

The managed fragment is
`~/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf`; `off` removes it and
the ordinary state under `~/.local/state/id14-sr/`. An unplugged OFF may leave
only a per-output `metadata-pending-*` file as described above. The installed user unit is
disabled and inactive until the next `on`.

## Primary specifications consulted

- PipeWire filter-chain module and virtual sink examples:
  <https://docs.pipewire.org/page_module_filter_chain.html>
- PipeWire loopback and explicit `target.object` examples:
  <https://docs.pipewire.org/page_module_loopback.html>
- PipeWire properties (`target.object`, `audio.rate`, scheduling latency):
  <https://docs.pipewire.org/page_man_pipewire-props_7.html>
- WirePlumber `wpctl set-default` behavior for new streams:
  <https://pipewire.pages.freedesktop.org/wireplumber/man/wpctl.html>
- WirePlumber per-stream `target.object` movement:
  <https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/settings.html>
- WirePlumber Smart Filter insertion, target matching, stacking, and metadata:
  <https://pipewire.pages.freedesktop.org/wireplumber/policies/smart_filters.html>
- WirePlumber 0.5 release history (Smart Filters were present and corrected in
  the 0.5 series):
  <https://pipewire.pages.freedesktop.org/wireplumber/resources/releases.html>
- PipeWire 1.6.8 `pw-metadata` source; `-d id key` deletes one property by
  passing a null value to `pw_metadata_set_property`:
  <https://gitlab.freedesktop.org/pipewire/pipewire/-/blob/1.6.8/src/tools/pw-metadata.c>
- Official LADSPA 1.1 ABI and header:
  <https://ladspa.org/ladspa_sdk/overview.html> and
  <https://www.ladspa.org/ladspa_sdk/ladspa.h.txt>
