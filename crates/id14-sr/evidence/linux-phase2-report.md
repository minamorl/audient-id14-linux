# Linux PipeWire phase-2 evidence

Date: 2026-10-02 JST
Target: operator-supplied `<linux-host>`, NixOS, PipeWire 1.6.8, Audient iD14 MKII

## Result and boundary

The self-built phase-1 model now runs as a Rust LADSPA plugin in two PipeWire
filter-chains inserted by WirePlumber Smart Filters in front of both iD14 UCM
outputs. Normal `id14-sr on` covers Line and Headphones together;
`on line|headphones` remains as a compatibility mode, and `off|status` covers
the whole managed graph.
It does not change the global default or application targets. Installation is
off by default. The final installed state after all tests was direct iD14 Line
output, service disabled/inactive, Smart Filter nodes absent, filter metadata
empty, and managed filter configuration absent.

The covered playback scope is normal PipeWire-native and Pulse-compat streams
whose default or explicit target is either iD14 UCM Line or Headphones. ALSA
`hw:`/`plughw:` direct access, raw six-channel or pro-audio profiles, and
JACK/direct PipeWire port connections bypass this user-space filter.

This phase does not alter the phase-1 checkpoint, source attribution, failed
model evidence, or heldout results. No corpus, MP3, WAV, or A/B audio was
transferred to Linux. The checkpoint remains 963,264 bytes.

## Primary specifications read before ABI/protocol implementation

- PipeWire filter-chain graph, LADSPA node, and virtual sink examples:
  <https://docs.pipewire.org/page_module_filter_chain.html>
- PipeWire loopback virtual sink and explicit `target.object` examples:
  <https://docs.pipewire.org/page_module_loopback.html>
- `target.object`, `audio.rate`, and node scheduling/latency properties:
  <https://docs.pipewire.org/page_man_pipewire-props_7.html>
- `wpctl set-default` applies to new auto-connected streams and persists:
  <https://pipewire.pages.freedesktop.org/wireplumber/man/wpctl.html>
- `linking.allow-moving-streams` moves existing streams through per-stream
  `target.object` metadata:
  <https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/settings.html>
- Smart Filters transparently insert a filter for a specific device; the main
  node uses `filter.smart = true` and a JSON `filter.smart.target` match:
  <https://pipewire.pages.freedesktop.org/wireplumber/policies/smart_filters.html>
- WirePlumber 0.5 release history, including early Smart Filter corrections:
  <https://pipewire.pages.freedesktop.org/wireplumber/resources/releases.html>
- PipeWire 1.6.8 `pw-metadata` implementation; `-d id key` invokes
  `pw_metadata_set_property` with a null value for that key only:
  <https://gitlab.freedesktop.org/pipewire/pipewire/-/blob/1.6.8/src/tools/pw-metadata.c>
- Official LADSPA 1.1 overview and ABI header:
  <https://ladspa.org/ladspa_sdk/overview.html> and
  <https://www.ladspa.org/ladspa_sdk/ladspa.h.txt>

The installed system also supplied the official `filter-chain.service`; its
`ExecStart` is `pipewire -c filter-chain.conf`. A short-lived `pw-cli
load-module` does not retain a server-side graph after that client exits, so
the final implementation uses a dedicated user service with the same
official filter-chain entrypoint.

## Implementation decisions

- The LADSPA instance owns all model, FFT, detector, and fixed buffer state.
  `run()` performs no allocation, blocking operation, file/device I/O, or
  logging. Construction and checkpoint decoding happen in `instantiate()`.
- Any planar host block length is adapted to 256-frame stereo interleaved
  chunks. The adapter adds exactly 256 frames to the model's 768 frames.
- Unsupported sample rates pass through without processing. The PipeWire
  nodes request 48 kHz explicitly.
- A 256-point Hann FFT compares 13.5–20 kHz energy with 0.75–12 kHz energy.
  Completion is full below ratio 0.0002, bypassed above 0.002, logarithmically
  blended between them, faded in by 0.125/chunk and released by 0.5/chunk.
  Silence is bypassed.
- The phase-1 `process` interface is unchanged. Additive `process_with_mix`
  provides continuous high-band preservation/completion blending and its 0/1
  endpoints are tested against the old API.
- Quantization was not introduced. On the target CPU the complete adapter is
  about 5.9% of one 256-frame period at p95, so quantization was unnecessary
  for latency/headroom and would have changed the already validated model.
- The final route uses WirePlumber Smart Filter policy rather than changing the
  default or manually moving application streams. Existing and new streams
  targeting either iD14 UCM sink are policy-linked through that output's
  filter; unrelated and explicitly targeted streams never change route.
- Line and Headphones are existing UCM loopback filters sharing
  `alsa_output.hw_iD14_0`. A private silent loopback node per output is used
  only as a distinct Smart Filter target identity. Marking both UCM filters
  Smart and ordering each after its `id14-sr-*` instance yields
  `application -> SR -> matching UCM filter -> raw iD14`; the private
  transports carry no application audio. Line maps to AUX2/3 and Headphones to
  AUX0/1.
- `ExecStartPost=id14-sr reconcile` waits for every requested UCM sink and
  reapplies dynamic filter metadata after a service or WirePlumber restart.
  Status requires nodes plus all owned metadata before reporting ON; repeated
  ON repairs missing metadata.
- Metadata has an `id14-sr.owner` marker. OFF deletes only its five keys with
  `pw-metadata -d id key`, preserving unrelated keys. It resolves the selected
  node by name and never deletes through a saved numeric ID alone. If the node
  is absent, service/config cleanup proceeds and `metadata-pending` retains the
  minimum information needed to retry safely after reconnect.

## Raw gates

### Mac workspace gate

Command:

```text
bash -n crates/id14-sr-ladspa/linux/id14-sr crates/id14-sr-ladspa/linux/install-user.sh crates/id14-sr-ladspa/linux/test-routing-scope.sh
shellcheck crates/id14-sr-ladspa/linux/id14-sr crates/id14-sr-ladspa/linux/install-user.sh crates/id14-sr-ladspa/linux/test-routing-scope.sh
cargo fmt --all -- --check
CARGO_HOME=/private/tmp/id14-sr-cargo-home cargo build --workspace
CARGO_HOME=/private/tmp/id14-sr-cargo-home cargo test --workspace
git diff --check
```

Exit: `0`

Output tail:

```text
test tests::adapter_and_engine_delay_is_exactly_1024_frames ... ok
test tests::detector_enables_completion_for_12k_bandlimited_material ... ok
test tests::detector_preserves_fullband_material_and_releases_quickly ... ok
test tests::arbitrary_host_blocks_match_fixed_host_blocks ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
...
FINAL_LOCAL_GATE syntax=0 shellcheck=0 fmt=0 build=0 test=0 diff=0
```

Shellcheck:

```text
$ shellcheck crates/id14-sr-ladspa/linux/id14-sr crates/id14-sr-ladspa/linux/install-user.sh crates/id14-sr-ladspa/linux/test-routing-scope.sh
SHELLCHECK_EXIT=0
```

### Linux build and unit gate

Command:

```text
nix shell nixpkgs#rustc nixpkgs#cargo --command bash -lc \
  "cargo test -p id14-sr -p id14-sr-ladspa && cargo build --release -p id14-sr-ladspa"
```

Exit: `0`

Output tail:

```text
running 6 tests
test tests::descriptor_has_one_stereo_plugin ... ok
test tests::unsupported_sample_rate_is_safe_passthrough ... ok
test tests::adapter_and_engine_delay_is_exactly_1024_frames ... ok
test tests::detector_enables_completion_for_12k_bandlimited_material ... ok
test tests::detector_preserves_fullband_material_and_releases_quickly ... ok
test tests::arbitrary_host_blocks_match_fixed_host_blocks ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
...
Finished `release` profile [optimized] target(s) in 6.26s
```

### LADSPA ABI load

Command:

```text
nix shell nixpkgs#ladspa-sdk --command analyseplugin \
  ~/.local/lib/ladspa/libid14_sr_ladspa.so
```

Exit: `0`

Output:

```text
Plugin Name: "iD14 Stereo High-Frequency Completion"
Plugin Label: "id14_sr_stereo"
Plugin Unique ID: 4801585
Maker: "audient-id14-linux contributors"
Copyright: "MIT"
Must Run Real-Time: No
Has activate() Function: No
Has deactivate() Function: No
Has run_adding() Function: No
Environment: Normal
This plugin cannot use in-place processing. It will not work with all hosts.
Ports:  "Input L" input, audio
        "Input R" input, audio
        "Output L" output, audio
        "Output R" output, audio

ANALYSE_EXIT=0
```

Direct ELF loading also returned a non-null descriptor for index 0 and null
for index 1; `nm -D` exported `ladspa_descriptor`.

### Ryzen 7 7700 release benchmarks

Commands:

```text
~/.local/src/id14-sr-phase2/target/release/sr-bench
~/.local/src/id14-sr-phase2/target/release/adapter-bench
```

Exit: `0`

Output:

```text
sample_rate=48000 frames=256 budget_us=5333.33 median_us=314.65 p95_us=322.64 max_us=416.34
sample_rate=48000 frames=256 budget_us=5333.33 adapter_delay_frames=256 median_us=315.03 p95_us=320.72 max_us=333.62
```

### Active playback, stream movement, idempotency, and graph errors

A 12-second 3 kHz stereo `s16le` stream was generated in a pipe and played by
`pacat`; no audio file was written. It started on direct Line, then `on line`
was run while it was active, `on line` was repeated, and `off line` was run
before playback ended.

Exit: `0`

Selected raw output:

```text
DIRECT_STREAM
136     pacat   4721
4721    4326    4720    PipeWire        s16le 2ch 48000Hz
ON_ACTIVE
on: output=line sink=id14_sr_sink service=active
pacat:output_FL
  |-> id14_sr_sink:playback_FL
pacat:output_FR
  |-> id14_sr_sink:playback_FR
id14_sr_output:output_FL
  |-> alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink:playback_FL
id14_sr_output:output_FR
  |-> alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink:playback_FR
IDEMPOTENT_ON
on: output=line sink=id14_sr_sink (already active)
PW_TOP_ACTIVE
R 184  2048 48000 2.8ms 25.0us 0.07 0.00 0 S32LE 6 48000 alsa_output.hw_iD14_0
R 46      0     0 13.9us 13.7us 0.00 0.00 0 F32P 2 48000 + id14_sr_sink
R 154     0     0  4.0us  2.7ms 0.00 0.06 0 F32P 2 48000 + id14_sr_output
R 184  2048 48000 2.6ms 12.6us 0.06 0.00 0 S32LE 6 48000 alsa_output.hw_iD14_0
R 154     0     0  2.8us  2.5ms 0.00 0.06 0 F32P 2 48000 + id14_sr_output
OFF_ACTIVE
off: output=line sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink service=inactive
OFF_ACTIVE_PULSE
Sink Input #4721
    Sink: 4326
    media.name = "id14-sr-test"
    node.name = "pacat"
RESULT on=0 off=0 playback=0
FINAL_SERVICE
inactive
ls: cannot access '.../90-id14-sr.conf': No such file or directory
```

The three active snapshots had PipeWire `ERR=0`; filter BUSY was 2.5–2.7 ms
inside a 2048-frame / 48 kHz period (42.67 ms). This is a graph-level xrun
check, not a subjective no-artifact listening claim.

### Latency

The impulse test measured the plugin/model peak at exactly frame 1024:

```text
test tests::adapter_and_engine_delay_is_exactly_1024_frames ... ok
```

At 48 kHz this is 21.33 ms. The active graph reported quantum 2048 at 48 kHz
and the filter sink `Latency` parameter reported one input quantum. Therefore
the measured runtime bound for delay added over direct playback is:

```text
DSP/adapter 1024/48000 + filter graph 2048/48000 = 64.00 ms
```

This is below the requested roughly 100 ms. It is a software graph/DSP
measurement, not an acoustic round-trip measurement through iD14 converters.

### Headphones route

Exit: `0`

```text
HEADPHONES_ON
on: output=headphones sink=id14_sr_sink service=active
physical_target=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
102     alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
id14_sr_output:output_FL
  |-> alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink:playback_FL
id14_sr_output:output_FR
  |-> alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink:playback_FR
HEADPHONES_OFF
off: output=headphones sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink service=inactive
```

### Missing-plugin rollback

Exit from attempted `on`: `1` (expected). The inspection command itself
exited `0` after recording the failure.

```text
id14-sr: plugin is unavailable: /definitely/missing/id14-sr.so
MISSING_PLUGIN_EXIT=1
state=off output=none service=inactive virtual_sink=id14_sr_sink virtual_id=none filter_output_id=none
inactive
ls: cannot access '.../90-id14-sr.conf': No such file or directory
Default Configured Devices:
    Audio/Sink alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
```

### Persistent on/off unit state

Exit: `0`

```text
PERSISTENCE_ON
on: output=line sink=id14_sr_sink service=active
enabled
active
PERSISTENCE_OFF
off: output=line sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink service=inactive
disabled
inactive
state=off output=none service=inactive virtual_sink=id14_sr_sink virtual_id=none filter_output_id=none
```

### Earlier manual-routing regression (superseded)

The regression script creates a temporary `module-null-sink`, starts Pulse
and native `pw-cat` playback on both that unrelated sink and the iD14 Line,
and inspects the actual PipeWire links by stream node ID. It covers already-off,
on, repeated on, off, and a manual default change while on. Its EXIT trap
stops and waits for every stream, disables the effect, restores the original
default, and unloads the temporary sink.

Command:

```text
~/.local/libexec/id14-sr-test-routing-scope
```

Exit: `0`

Selected raw output:

```text
BEFORE_ALREADY_OFF default=id14_sr_test_null
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-native-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
off: already inactive; no changes
AFTER_ALREADY_OFF default=id14_sr_test_null
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-native-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
on: output=line sink=id14_sr_sink service=active
AFTER_ON default=id14_sr_sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=id14_sr_sink
scope-native-id14 sink=id14_sr_sink
on: output=line sink=id14_sr_sink (already active)
AFTER_IDEMPOTENT_ON default=id14_sr_sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=id14_sr_sink
scope-native-id14 sink=id14_sr_sink
default-restored: sink=id14_sr_test_null
off: output=line sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink service=inactive
AFTER_OFF default=id14_sr_test_null
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-native-id14 sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
default-preserved: sink=id14_sr_test_null
AFTER_MANUAL_DEFAULT_OFF default=id14_sr_test_null
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
scope-pulse-id14 sink=id14_sr_test_null
scope-native-id14 sink=id14_sr_test_null
ROUTING_SCOPE_TEST=PASS line_id=196 null_id=157 module_id=536870916
CLEANUP effect=off service=inactive null_sink=absent default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
```

The last branch deliberately changes the default from the virtual sink to the
null sink while the effect is on. WirePlumber consequently relinks all four
test streams to null; `off` preserves that manual default and, because none is
then currently linked to the plugin, leaves all four on null as required.

The Smart Filter implementation below supersedes this default-switching and
manual-movement implementation. This output remains as failure-history and
regression context.

### Final Smart Filter routing on WirePlumber 0.5.15

The runtime reports `wireplumber 0.5.15`. Its shipped
`linking/get-filter-from-target.lua` and `lib/filter-utils.lua` were inspected
from the exact Nix store path. The iD14 Line and Headphones UCM nodes are
themselves non-Smart loopback filters with separate link groups but the same
raw `alsa_output.hw_iD14_0` target. A direct Smart target match on Line was
recognized in node properties but inserted neither existing nor new streams.
Making the raw ALSA node the common target did insert the filter, but also
captured Headphones and sent it toward Line, so that unsafe graph was rejected.

The final graph gives the selected UCM filter a private target identity and
orders it after the SR filter. Two full invocations of
`linux/test-routing-scope.sh` exited `0`. Each invocation covered six
simultaneous streams: Pulse/native explicit Line, Pulse/native default Line,
and Pulse/native explicit unrelated null sink; the first cycle started streams
before ON and the second after ON. It also covered repeated ON, service restart,
OFF, simulated missing physical sink cleanup, timestamped unrelated-route
monitoring, null module unload, and process cleanup.

Command (run twice):

```text
scp crates/id14-sr-ladspa/linux/test-routing-scope.sh \
  <linux-host>:/tmp/id14-sr-test-routing-scope.sh
ssh <linux-host> \
  '/tmp/id14-sr-test-routing-scope.sh; rc=$?; rm -f /tmp/id14-sr-test-routing-scope.sh; exit $rc'
```

Exit: `0` both times.

Selected raw output from run 2:

```text
PRE_COMPREHENSIVE_2
state=off output=none service=inactive smart_main=id14_sr_sink main_id=none stream_id=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
config=absent
Found "filters" metadata 44
CYCLE1_EXISTING_STREAMS
scope-pulse-explicit sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-native-explicit sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-pulse-default sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-native-default sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
on: output=line smart_target=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink target_id=184 main_id=194 stream_id=106 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink service=active
AFTER_ON_1 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-pulse-explicit sink=id14_sr_sink
scope-native-explicit sink=id14_sr_sink
scope-pulse-default sink=id14_sr_sink
scope-native-default sink=id14_sr_sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
id14_sr_output sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
on: output=line smart_target=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink main_id=194 stream_id=106 (already active)
enabled
AFTER_RESTART_1 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
scope-pulse-explicit sink=id14_sr_sink
scope-native-explicit sink=id14_sr_sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
off: output=line physical=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink availability=available service=inactive
SCOPE_MONITOR label=cycle1 samples=33 first_ns=1790947507389084756 last_ns=1790947510054695901 violations=0
CYCLE2_NEW_STREAMS_WHILE_ON
scope-pulse-explicit sink=id14_sr_sink
scope-native-explicit sink=id14_sr_sink
scope-pulse-default sink=id14_sr_sink
scope-native-default sink=id14_sr_sink
scope-pulse-null sink=id14_sr_test_null
scope-native-null sink=id14_sr_test_null
SCOPE_MONITOR label=cycle2 samples=27 first_ns=1790947510194270901 last_ns=1790947512249155566 violations=0
OFFLINE_SIMULATION
id14-sr: warning: physical sink unavailable; SR-bound streams cannot be returned: scope-native-default,pacat,scope-native-explicit,pacat
off: output=line physical=id14_sr_missing_for_test availability=unavailable service=inactive
SCOPE_MONITOR label=offline samples=22 first_ns=1790947512494942156 last_ns=1790947514200800456 violations=0
SMART_FILTER_SCOPE_TEST=PASS default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink module_id=536870916
CLEANUP effect=off service=inactive null_sink=absent default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink processes=none
```

The selected-output isolation check also ran both Pulse and native streams on
Headphones while Line streams played simultaneously. Exit: `0`.

```text
HEADPHONES_ROUTES
hp-pulse=id14_sr_sink
hp-native=id14_sr_sink
line-pulse=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-native=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
sr-output=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
off: output=headphones physical=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink availability=available service=inactive
HP_CLEANUP
state=off output=none service=inactive smart_main=id14_sr_sink main_id=none stream_id=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
Found "filters" metadata 44
```

Already-OFF was also tested while an unrelated null sink was the default and
held both Pulse and native streams. Exit: `0`; cleanup restored Line and
unloaded the null sink.

```text
off_output=off: already inactive; no changes
before default=id14_sr_test_off_null pulse=id14_sr_test_off_null native=id14_sr_test_off_null
after default=id14_sr_test_off_null pulse=id14_sr_test_off_null native=id14_sr_test_off_null
ALREADY_OFF_CLEANUP
state=off output=none service=inactive smart_main=id14_sr_sink main_id=none stream_id=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
```

### Earlier final installed state (superseded)

Exit: `0`

```text
FINAL_INSTALLED_STATUS
state=off output=none service=inactive virtual_sink=id14_sr_sink virtual_id=none filter_output_id=none plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
SERVICE
inactive
disabled
DEFAULT
  * node.name = "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink"
CONFIG_AND_STATE
absent /home/minamorl/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
absent /home/minamorl/.local/state/id14-sr/output
absent /home/minamorl/.local/state/id14-sr/prior-default
absent /home/minamorl/.local/libexec/id14-sr-test-routing-scope
GRAPH_RESIDUE
MODULE_RESIDUE
PROCESS_RESIDUE
INSTALLED_SHA
25e6ca15dec6f384c38222f6a038fee9f55858d1c9bbc963d751804991aedd33  /home/minamorl/.local/bin/id14-sr
3dcc5fce3bbe618a0114209895b6126c458f180238d0b39301e49820637e6e07  /home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
```

### Final Smart Filter installed state and cleanup

Exit: `0`.

```text
FINAL_INSTALLED_STATUS
state=off output=none service=inactive smart_main=id14_sr_sink main_id=none stream_id=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
SERVICE
inactive
disabled
ActiveState=inactive
UnitFileState=disabled
Result=success
DEFAULT
  * node.name = "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink"
FOLLOW_DEFAULT
Value: true
CONFIG_STATE
absent /home/minamorl/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
absent /home/minamorl/.local/state/id14-sr/output
absent /home/minamorl/.local/state/id14-sr/physical-id
FILTER_METADATA
Found "filters" metadata 44
GRAPH_RESIDUE
MODULE_RESIDUE
PROCESS_RESIDUE
TEMP_RESIDUE
INSTALLED_SHA
2f0a4d3ed1a0bfcdff51f9b56398637cde8bbabd0131a453c81491d0e2e738ab  /home/minamorl/.local/bin/id14-sr
9289a2083d98839b5d381e9e9974c5ba897533063cba6a3d63e30b7f1b8bd01c  /home/minamorl/.config/systemd/user/id14-sr-filter.service
3dcc5fce3bbe618a0114209895b6126c458f180238d0b39301e49820637e6e07  /home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
INSTALLED_LS
-rw-r--r-- 1 minamorl users     571 10月  2 22:23 /home/minamorl/.config/systemd/user/id14-sr-filter.service
-rwxr-xr-x 1 minamorl users   14643 10月  2 22:28 /home/minamorl/.local/bin/id14-sr
-rwxr-xr-x 1 minamorl users 3667200 10月  2 21:39 /home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
```

Final unit and ABI checks also exited `0`:

```text
Plugin Name: "iD14 Stereo High-Frequency Completion"
Plugin Label: "id14_sr_stereo"
Plugin Unique ID: 4801585
Ports:  "Input L" input, audio
        "Input R" input, audio
        "Output L" output, audio
        "Output R" output, audio
REMOTE_FINAL_GATE unit_verify=0 abi=0
```

### Metadata ownership and manager-recreation correction

The final regression now removes one owned key while the nodes remain active,
requires `status` to report degraded, repairs it through idempotent ON, and
verifies that a sentinel from another application survives OFF. It then
restarts the actual WirePlumber manager while six streams are active. Exit:
`0`.

```text
STATUS_AFTER_METADATA_LOSS
state=degraded output=line service=active metadata=missing smart_main=id14_sr_sink main_id=171 stream_id=210 transport_id=183 transport_stream_id=70 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
IDEMPOTENT_ON_RECOVERY
on: output=line smart_target=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink main_id=171 stream_id=210 (metadata recovered)
SCOPE_MONITOR label=cycle1 samples=42 first_ns=1790948299633529834 last_ns=1790948302999885264 violations=0
WIREPLUMBER_RESTART_RECONCILE
WIREPLUMBER_IDS before_pid=189745 after_pid=191963 before_line=77 after_line=53
state=on output=line service=active metadata=ok smart_main=id14_sr_sink main_id=114 stream_id=67 transport_id=160 transport_stream_id=216 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
SCOPE_MONITOR label=cycle2 samples=29 first_ns=1790948306198595038 last_ns=1790948308351147008 violations=0
off: output=line physical=id14_sr_missing_for_test availability=unavailable metadata_cleanup=pending-unavailable service=inactive
OFFLINE_DEFERRED_CLEANUP
off: output=line physical=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink availability=available metadata_cleanup=removed service=inactive
SCOPE_MONITOR label=offline samples=30 first_ns=1790948308588359777 last_ns=1790948310888419244 violations=0
SMART_FILTER_SCOPE_TEST=PASS default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink module_id=536870916
CLEANUP effect=off service=inactive null_sink=absent default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink processes=none
```

The focused sentinel test shows the exact post-OFF metadata. Exit: `0`.

```text
STATUS_AFTER_OWNED_KEY_LOSS
state=degraded output=line service=active metadata=missing smart_main=id14_sr_sink main_id=75 stream_id=190 transport_id=129 transport_stream_id=123 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
IDEMPOTENT_ON_RECOVERY
on: output=line smart_target=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink main_id=75 stream_id=190 (metadata recovered)
OFF_WITH_SENTINEL
off: output=line physical=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink availability=available metadata_cleanup=removed service=inactive
Found "filters" metadata 44
update: id:196 key:'id14-sr.test-sentinel' value:'keep' type:'Spa:String'
SENTINEL_SELECTIVE_DELETE_TEST=PASS
```

A null sink then stood in for a different node reusing the saved numeric ID.
The selected physical name was deliberately unavailable. OFF left every key,
including the sentinel, untouched and retained pending cleanup. After the
test removed only its simulated owned keys, normal OFF cleared pending state
without deleting the sentinel. Exit: `0`.

```text
SIM_IDS line=196 reused=177
off: output=line physical=id14_sr_missing_for_stale_test availability=unavailable metadata_cleanup=pending-unavailable service=inactive
AFTER_STALE_OFF
update: id:177 key:'id14-sr.test-sentinel' value:'keep' type:'Spa:String'
update: id:177 key:'id14-sr.owner' value:'id14-sr-v1' type:'Spa:String'
update: id:177 key:'filter.smart' value:'true' type:'Spa:Bool'
update: id:177 key:'filter.smart.name' value:'id14-output-line' type:'Spa:String'
update: id:177 key:'filter.smart.target' value:'{ node.name = "id14_sr_transport" }' type:'Spa:String'
update: id:177 key:'filter.smart.after' value:'[ "id14-sr-line" ]' type:'Spa:String'
STALE_ID_WAS_NOT_DELETED=PASS
off: output=line physical=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink availability=available metadata_cleanup=absent service=inactive
STALE_ID_GUARD_TEST=PASS
```

The raw hardware port check and active callback sample both exited `0`:

```text
AUX_LINKS output=line
output_AUX2->playback_AUX2
output_AUX3->playback_AUX3
R  177      0      0  31.3us  17.3us  0.00  0.00    0     F32P 2 48000  + id14_sr_sink
R  227      0      0   5.3us 106.3us  0.00  0.00    0     F32P 2 48000  + id14_sr_output
AUX_LINKS output=headphones
output_AUX0->playback_AUX0
output_AUX1->playback_AUX1
R  160      0      0   7.2us   5.4us  0.00  0.00    0     F32P 2 48000  + id14_sr_sink
R   38      0      0   2.2us  70.8us  0.00  0.00    0     F32P 2 48000  + id14_sr_output
AUX_PAIR_AND_CALLBACK_TEST=PASS
```

Final correction readback exited `0`:

```text
FINAL_CORRECTION_STATUS
state=off output=none service=inactive metadata=none smart_main=id14_sr_sink main_id=none stream_id=none transport_id=none transport_stream_id=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
ALREADY_OFF
off: already inactive; no changes
SERVICE
inactive
disabled
ActiveState=inactive
UnitFileState=disabled
Result=success
DEFAULT
  * node.name = "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink"
FOLLOW_DEFAULT
Value: true
CONFIG_STATE
absent /home/minamorl/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
absent /home/minamorl/.local/state/id14-sr/output
absent /home/minamorl/.local/state/id14-sr/physical-id
absent /home/minamorl/.local/state/id14-sr/metadata-pending
FILTER_METADATA
Found "filters" metadata 95
GRAPH_RESIDUE
MODULE_RESIDUE
PROCESS_RESIDUE
TEMP_RESIDUE
INSTALLED_SHA
788b5a8ffeed673f5fa888a0fd904aea46577b0a5f15992c5c931348cd28b596  /home/minamorl/.local/bin/id14-sr
9289a2083d98839b5d381e9e9974c5ba897533063cba6a3d63e30b7f1b8bd01c  /home/minamorl/.config/systemd/user/id14-sr-filter.service
3dcc5fce3bbe618a0114209895b6126c458f180238d0b39301e49820637e6e07  /home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
```

```text
CORRECTION_REMOTE_GATE unit_verify=0 abi=0
```

### Dual-output pin and atomic-activation correction

The final default command is `id14-sr on` (mode `all`). It creates independent
Line and Headphones filter/transport pairs. Compatibility modes
`id14-sr on line` and `id14-sr on headphones` remain available. The source
CLI was installed before this test; its SHA-256 was
`033efe81aee5f54004c4fb103038e1d8d89051c123509e0191328ece36e446b3`.

The permanent routing test was run on the NixOS user session:

```text
$ ssh <linux-host> ~/.local/libexec/id14-sr-test-routing-scope
ATOMIC_ROLLBACK_TESTS
ROLLBACK stage=transport exit=1 output=... simulated transport verification failure; activation rolled back
ROLLBACK stage=metadata exit=1 output=... simulated metadata verification failure; activation rolled back
CYCLE1_EXISTING_STREAMS
BEFORE_ON_1 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-pulse sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-native sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
headphones-pulse sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
headphones-native sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
default-pulse sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
default-native sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
null-pulse sink=id14_sr_test_null
null-native sink=id14_sr_test_null
on: requested=all active_outputs=line,headphones ... service=active
AFTER_ON_1 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-pulse sink=id14_sr_sink
line-native sink=id14_sr_sink
headphones-pulse sink=id14_sr_headphones_sink
headphones-native sink=id14_sr_headphones_sink
default-pulse sink=id14_sr_sink
default-native sink=id14_sr_sink
null-pulse sink=id14_sr_test_null
null-native sink=id14_sr_test_null
id14_sr_output sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
id14_sr_headphones_output sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
IDEMPOTENT_ON=on: requested=all active_outputs=line,headphones (already active)
STATUS_METADATA_LOSS
state=degraded requested=all service=active active_outputs=headphones degraded_outputs=line ...
RECOVERY=reconciled: active_outputs=line,headphones
on: requested=all active_outputs=line,headphones (metadata recovered)
SERVICE_RESTART
state=on requested=all service=active active_outputs=line,headphones ...
WIREPLUMBER_RESTART
WIREPLUMBER_IDS before_pid=191963 after_pid=204166 line=53/66 headphones=121/164
state=on requested=all service=active active_outputs=line,headphones ...
OFF_SENTINEL_PRESERVATION
off: line_metadata=removed headphones_metadata=removed service=inactive
CYCLE2_NEW_STREAMS_WHILE_ON
AFTER_ON_2 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-pulse sink=id14_sr_sink
line-native sink=id14_sr_sink
headphones-pulse sink=id14_sr_headphones_sink
headphones-native sink=id14_sr_headphones_sink
default-pulse sink=id14_sr_sink
default-native sink=id14_sr_sink
null-pulse sink=id14_sr_test_null
null-native sink=id14_sr_test_null
off: line_metadata=removed headphones_metadata=removed service=inactive
AFTER_OFF_2 default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
line-pulse sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
headphones-pulse sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
null-pulse sink=id14_sr_test_null
null-native sink=id14_sr_test_null
SMART_FILTER_DUAL_SCOPE_TEST=PASS default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink module_id=536870916
CLEANUP status=state=off requested=none service=inactive ... null_sink=absent ... processes=none
```

Exit: `0`. The two injected failures occurred after the service had been
started and after metadata had been applied, respectively. After each exit the
service was inactive/disabled and managed config, state, nodes, and owned
metadata were absent. The default remained Line.

The unplugged/stale-ID simulation used an absent Line name while its saved
numeric ID still named the live Line node. OFF did not delete through that ID:

```text
SIMULATED_UNPLUG_OFF
off: line_metadata=pending-unavailable headphones_metadata=removed service=inactive
state=off requested=none service=inactive ... pending_outputs=line ...
PENDING=66
LINE_METADATA_RETAINED
update: id:66 key:'id14-sr.owner' value:'id14-sr-v2-line' type:'Spa:String'
update: id:66 key:'id14-sr.test-sentinel' value:'stale-keep' type:'Spa:String'
CONFIG=absent
NORMAL_IDENTITY_CLEANUP
off: line_metadata=removed headphones_metadata=absent service=inactive
update: id:66 key:'id14-sr.test-sentinel' value:'stale-keep' type:'Spa:String'
state=off requested=none service=inactive ... pending_outputs=none ...
```

Exit: `0`. Thus cleanup is deferred when identity is unavailable, and a later
OFF with the named physical node removes only owned keys while preserving the
sentinel.

Concurrent Line and Headphones playback produced these raw hardware links and
active callback rows:

```text
DUAL_AUX_LINKS
id14_sr_headphones_transport_output output_AUX0 playback_AUX0
id14_sr_headphones_transport_output output_AUX1 playback_AUX1
id14_sr_transport_output            output_AUX2 playback_AUX2
id14_sr_transport_output            output_AUX3 playback_AUX3
PW_TOP_CALLBACK
R  216  0  0  20.3us  5.1us  0.00  0.00  0  F32P 2 48000  + id14_sr_sink
R  113  0  0  87.0us 80.9us  0.00  0.00  0  F32P 2 48000  + id14_sr_output
R   73  0  0  17.7us  7.7us  0.00  0.00  0  F32P 2 48000  + id14_sr_headphones_sink
R   69  0  0   8.4us 82.7us  0.00  0.00  0  F32P 2 48000  + id14_sr_headphones_output
CALLBACK_TEST=PASS
```

Exit: `0`; `ERR` is zero for all four active filter nodes. Each stream still
has the previously measured per-route bound of 64.00 ms (21.33 ms exact
DSP/adapter delay plus one 2048-frame/48 kHz graph quantum), below 100 ms. The
second filter is parallel and does not add serial latency to either route.

Only the two Rust crates, their workspace/lock files, and the embedded 941 KiB
checkpoint were transferred for the final Linux rebuild; no corpus or audio
was transferred. The rebuild and ABI load exited `0`:

```text
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Finished `release` profile [optimized] target(s) in 15.99s
BUILT_PLUGIN=3dcc5fce3bbe618a0114209895b6126c458f180238d0b39301e49820637e6e07
LINUX_BENCH
sample_rate=48000 frames=256 budget_us=5333.33 adapter_delay_frames=256 median_us=315.71 p95_us=323.12 max_us=403.54
Plugin Label: "id14_sr_stereo"
Ports: "Input L" input, audio ... "Output R" output, audio
```

The rebuilt plugin hash exactly matched the installed plugin. The build tree,
source archive, and installed temporary test runner were removed afterward.

The two compatibility modes were also started and stopped independently;
exit was `0`:

```text
COMPAT_LINE
on: requested=line active_outputs=line ... service=active
state=on requested=line service=active active_outputs=line degraded_outputs=none ...
off: line_metadata=removed headphones_metadata=absent service=inactive
COMPAT_HEADPHONES
on: requested=headphones active_outputs=headphones ... service=active
state=on requested=headphones service=active active_outputs=headphones degraded_outputs=none ...
off: line_metadata=absent headphones_metadata=removed service=inactive
COMPAT_FINAL
state=off requested=none service=inactive active_outputs=none degraded_outputs=none pending_outputs=none ...
disabled
```

Final Linux readback exited `0`. Calling OFF again was a true no-op:

```text
ALREADY_OFF
off: already inactive; no changes
FINAL_STATUS
state=off requested=none service=inactive active_outputs=none degraded_outputs=none pending_outputs=none default=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink plugin=/home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
line: physical_id=66 main_id=none stream_id=none transport_id=none transport_stream_id=none
headphones: physical_id=164 main_id=none stream_id=none transport_id=none transport_stream_id=none
SERVICE
inactive
disabled
ActiveState=inactive
UnitFileState=disabled
Result=success
DEFAULT
  * node.name = "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink"
FOLLOW_DEFAULT
Value: true
FILTER_METADATA
Found "filters" metadata 115
OWNED_METADATA
none
CONFIG_STATE
absent /home/minamorl/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
absent /home/minamorl/.local/state/id14-sr/output
absent /home/minamorl/.local/state/id14-sr/physical-id-line
absent /home/minamorl/.local/state/id14-sr/physical-id-headphones
absent /home/minamorl/.local/state/id14-sr/metadata-pending-line
absent /home/minamorl/.local/state/id14-sr/metadata-pending-headphones
GRAPH_RESIDUE
MODULE_RESIDUE
none
PROCESS_RESIDUE
pacat=none
pw-cat=none
TEMP_RESIDUE
test_runner=absent
INSTALLED_HASHES
033efe81aee5f54004c4fb103038e1d8d89051c123509e0191328ece36e446b3  /home/minamorl/.local/bin/id14-sr
9289a2083d98839b5d381e9e9974c5ba897533063cba6a3d63e30b7f1b8bd01c  /home/minamorl/.config/systemd/user/id14-sr-filter.service
3dcc5fce3bbe618a0114209895b6126c458f180238d0b39301e49820637e6e07  /home/minamorl/.local/lib/ladspa/libid14_sr_ladspa.so
```

## Failures encountered

1. The first arbitrary-block test incorrectly assumed that generated harmonic
   phase must preserve left/right sign inversion. Doubling phase makes that
   assumption false. The test was replaced with the actual invariant: output
   is identical regardless of host block partitioning.
2. The first dynamic load attempt used `pw-cli load-module`. ABI loading was
   valid, but the short-lived client did not retain a graph. The installed
   official filter-chain service established the correct lifecycle model.
3. The first status query printed `null` for an absent node. The jq lookup now
   maps absence to no output and status reports `none`.
4. The first Pulse movement loop attempted to move the filter output itself;
   Pulse rejected it with `Invalid argument`, preventing feedback. The final
   code explicitly excludes the filter output node; the active
   playback rerun produced no warning and routed correctly.
5. Review found that the first general movement loop selected every playback
   stream. It could steal streams from unrelated devices and `off` while off
   could change the default. The intermediate replacement selected routes from actual
   PipeWire links, snapshots around default changes, and has the live null-sink
   regression above.
6. The first scoped regression rerun correctly preserved streams after a
   manual default change, but the test incorrectly expected streams that
   WirePlumber had already moved to null to return to Line. The oracle now
   follows the requirement: `off` moves only streams currently linked to the
   virtual sink. Cleanup succeeded on the failed attempt before rerunning; the
   final Smart Filter graph supersedes manual movement entirely.
7. The first final readback used `path` as a zsh loop variable, overwriting
   zsh's special command-search array and making the remaining read-only
   checks exit 127. Repeating the unchanged readback with variable `item`
   exited 0 and produced the final state above.
8. A direct Smart Filter whose target was the iD14 Line node exposed the
   correct `filter.smart` properties but WirePlumber 0.5.15 kept both existing
   and newly created explicit streams linked directly to Line. Runtime source
   inspection showed that Line is already a non-Smart filter; target discovery
   stops rather than wrapping such a filter.
9. Marking Line Smart and targeting the shared raw ALSA node made explicit
   Pulse/native Line streams traverse SR, but also captured the Headphones UCM
   split because both outputs share that node. The unsafe graph was cleaned up
   and rejected. A private per-selection target identity fixed the ambiguity.
10. The first Headphones isolation diagnostic printed the four correct route
    values, then exited `2` because of a quote error in an additional graph
    display expression. Its EXIT trap stopped/waited every stream and disabled
    the service. A corrected diagnostic was run from a clean OFF state and
    exited `0` with the output recorded above.
11. The first true WirePlumber restart stopped and restarted the PartOf unit,
    but `ExecStartPost` ran before the UCM Line sink was recreated and exited
    with `iD14 line sink is unavailable`. Cleanup left OFF/default Line/no test
    processes. Reconcile now waits up to five seconds for the selected sink;
    the rerun changed both manager and node IDs, restored all six routes and
    metadata, and exited `0`.
12. Independent verification found that the first Smart Filter default covered
    only one selected UCM output, allowing simultaneous playback on the other
    iD14 output to bypass SR. Default ON now owns two independent filter and
    private-transport pairs; an eight-stream live test covers both outputs and
    an unrelated null sink before and after ON/OFF and manager recreation.
13. The earlier ON path could exit after service startup or partial metadata
    application without undoing managed state. All post-mutation failures now
    call one checked rollback path. Fault injection after service/transport
    verification and after metadata verification produced nonzero exits and
    clean OFF state in both cases.

## Remaining risks

- The detector has synthetic full-band and 12 kHz-band-limited coverage, but
  naturally dark/full-band music can still be misclassified. The conservative
  fast-release blend limits replacement but does not prove subjective quality.
- The final dual-output routing test was 22 seconds, plus a separate active
  callback sample. `ERR=0` and process exit 0 do not replace a longer
  listening/soak test.
- The 64 ms result is the software graph plus exact DSP delay. No acoustic
  loopback measurement includes DAC/ADC latency.
- Phase-1 heldout evidence remains the quality basis: 35.32% high-band
  log-spectral error reduction versus zero extension and 16.59% versus fixed
  copy, with 48/51 and 44/51 tracks improved respectively. It is not a
  subjective quality claim.
- The final policy graph is verified on WirePlumber 0.5.15. It deliberately
  relies on that release's Smart Filter metadata and stacking behavior; a
  WirePlumber upgrade should rerun `linux/test-routing-scope.sh` before relying
  on automatic insertion.
- This user-space integration covers ordinary PipeWire-native and Pulse-compat
  playback targeting the iD14 UCM Line or Headphones nodes, including default
  and explicit targets. ALSA `hw:`/`plughw:` direct clients, raw six-channel or
  pro-audio profiles, and JACK/direct PipeWire port connections bypass it.
- If hardware disappears while ON, OFF cannot safely delete metadata through
  the saved numeric ID because that ID may now name another node. It therefore
  leaves a small per-output `metadata-pending-*` marker until the affected named node
  returns or WirePlumber recreates its metadata. Service and PipeWire config
  are still stopped/removed immediately, and the condition is visible in
  status/output rather than hidden.
