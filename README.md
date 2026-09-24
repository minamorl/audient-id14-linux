# audient-id14-linux

Linux control tool for the **Audient iD14 MKII** (mk2, USB `2708:0008`)
audio interface.

The **iD14 MKII (mk2)** is the primary target: it is the project owner's unit,
and it is what the default target, CLI auto-detection priority, udev rule
order, test fixtures, and any future hardware capture refer to. The original
**iD14 (mk1, USB `2708:0002`)** is also supported in the product definition
table, but every mk1 value in this project is **static-analysis inferred only
and hardware-unverified** — no mk1 unit is available to the project.

Audio streaming on Linux already works through the kernel's `snd-usb-audio`
driver. What is missing is a way to drive the device's *control* features
(monitor/mixer state, routing, etc.). This project aims to fill that gap with:

- `id14-protocol` — a pure, host-independent Rust library that builds and
  interprets iD14 control packets (unit-tested without hardware).
- `id14ctl` — a small command-line tool on top of it (device listing, product
  identification, read-only state dump, a gated volume write, and a
  `--dry-run` mode that prints the bytes it would send without touching the
  USB bus). See [Usage](#usage).

The tool is designed to coexist with `snd-usb-audio`; it does not replace the
audio driver and does not require unloading it. See
[Linux audio coexistence](#linux-audio-coexistence) below.

## Disclaimer

**This is an unofficial project. It is not affiliated with, endorsed by, or
supported by Audient in any way. "Audient" and "iD14" are trademarks of their
respective owner. Use this software at your own risk: sending control requests
to a USB audio interface may leave it in an unexpected state, and the authors
accept no liability for any damage or data loss.**

## Status

Work in progress. Hardware evidence so far (see
[`docs/protocol.md`](docs/protocol.md), recorded per row):

- **mk2, 2026-09-24:** the device descriptors were read and **GET-direction**
  requests (GET CUR / GET RANGE) were observed. This fixed the control path
  (UAC2 class requests through the DFU interface), the `wValue` / `wIndex`
  layout, and the entity IDs of the standard controls.
- **Not verified yet:** any SET request (volume writes), and any operation on
  **Linux**. Use [`tools/verify-linux.sh`](#verifying-on-ubuntu-read-only) to
  collect the read-only Linux evidence.
- **mk1:** static-analysis inferred only, hardware-unverified. It stays that
  way until someone with an mk1 unit examines one.

The protocol as a whole is still documented as a static-analysis inference;
only the rows marked `mk2 hardware-observed 2026-09-24` have hardware
evidence.

## Usage

```sh
id14ctl list                     # enumerate connected iD14 devices (mk2 first)
id14ctl info                     # identify the product (mk2 / mk1) by PID
id14ctl dump                     # read-only: show the declared standard controls
id14ctl --dry-run <command> ...  # print the bytes that would be sent; no USB transfer
id14ctl --enable-write volume ...  # write the output volume (dB); disabled by default
```

- `list`, `info` and `dump` are **read-only**. `dump` reads (GET CUR, and
  GET RANGE where needed) only the standard controls the device declares in its
  descriptor: clock source `SAM_FREQ`, clock selector, the declared Feature Unit
  controls, and the mixer unit. It never sends SET and does not read the
  Audient Extension Unit vendor controls (their width is not known yet).
- `volume` sets the volume control of the Feature Unit on the mixer's output
  side (FU `0x0C` on the mk2) with SET CUR. The value is given in **dB**; a value
  outside the device's GET RANGE (−127 dB … 0 dB on the mk2) is rejected without
  being sent. After writing, the value is read back with GET CUR and shown.
  **Writes are disabled unless `--enable-write` is given**, and SET is not yet
  verified on hardware.
- `mute` fails with a message that the device declares no mute control — the
  mk2 declares none. It does not fake mute by setting minimum volume.
- `--dry-run` prints the 8-byte setup packet and the payload that would be
  sent, and performs no USB transfer.
- `request` is a low-level control request command; see
  `id14ctl request --help`. Combine it with `--dry-run` to see the bytes
  without sending them.

## Linux audio coexistence

This tool does **control only**. The description is based on the mk2; the
same intent applies to mk1, unverified.

- Audio streaming stays with the kernel's `snd-usb-audio` driver, which holds
  the mk2's audio interfaces **0–2**. The tool **coexists** with it: it does
  **not** `rmmod` / unload `snd-usb-audio`, it does **not** claim interfaces
  0–2, and it does **not** detach any kernel driver.
- Control requests go to the device as UAC2 class requests through the **DFU
  interface** (class `0xFE`, interface 4 on the mk2), which `id14ctl` finds in
  the descriptor and is the only interface it claims. If the device has no DFU
  interface, `id14ctl` stops with an error rather than falling back to
  interface 0.
- The ALSA UCM profile shipped in `alsa-ucm-conf` for this device,
  `Audient-iD14-0008.conf`, targets the mk2 (PID `0x0008`). Interference-free
  operation alongside that profile is the coexistence baseline this project
  describes. No UCM profile exists for the mk1 (PID `0x0002`); how to handle
  that is not decided.

## Building

```sh
cargo build --workspace
cargo test --workspace
```

`id14ctl` will require `libusb-1.0` development headers once its USB backend
lands (`sudo apt-get install libusb-1.0-0-dev pkg-config` on Debian/Ubuntu).

## Verifying on Ubuntu (read-only)

`tools/verify-linux.sh` collects read-only evidence on a Linux machine with the
iD14 MKII attached. It performs five checks and prints PASS / FAIL for each:

1. `lsusb` descriptor dump of `2708:0008` (falls back to `2708:0002`), including
   the DFU interface (class `0xFE`).
2. `snd-usb-audio` is the driver bound to interfaces 0, 1 and 2 (read from
   sysfs).
3. `id14ctl list` output.
4. `id14ctl info` output.
5. `id14ctl dump` output.

The script is read-only: it never runs a write command, never passes
`--enable-write`, and never loads, unloads, binds or unbinds a driver. It
looks for `id14ctl` in `$ID14CTL`, then `PATH`, then `target/release` and
`target/debug`.

```sh
cargo build --release
tools/verify-linux.sh | tee verify-linux.log
```

## udev rules

`udev/99-audient-id14.rules` grants non-root access to both iD14 variants;
the mk2 (`2708:0008`) rules come first, then mk1 (`2708:0002`).
Copy it to `/etc/udev/rules.d/` and reload udev. The access mechanism
(`plugdev` group vs. `uaccess` tag) is provisional; see the comments in the
file.

## Documentation

- [`docs/protocol.md`](docs/protocol.md) — product definition table, control
  request format (`bmRequestType` / `bRequest` / `wValue` / `wIndex` /
  payload), control path, declared controls, and the extension-unit table
  (`getExtensionCode` ↔ UAC2 `wExtensionCode` ↔ mk2 unit ID). Evidence is
  recorded **per row**, with "inferred (static)" and "hardware-confirmed" in
  separate columns.
- [`docs/provenance.md`](docs/provenance.md) — where the protocol information
  came from (versions, hashes, referenced symbols, the 2026-09-24 mk2
  observation) and the MixiD cross-check results.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option (`MIT OR Apache-2.0`).

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

## 日本語

Audient iD14 MKII (mk2, USB `2708:0008`) を Linux から制御するための非公式
ツールです。主対象は mk2 (プロジェクトオーナーの実機) で、初代 iD14 (mk1,
USB `2708:0002`) も製品定義表に含めて対応しますが、mk1 の値はすべて静的解析
からの推定のみで、実機未照合です。
音声のストリーミングは既存の `snd-usb-audio` に任せ (アンロードしません・
interface 0-2 を claim しません・kernel driver を detach しません)、本プロジェクトは
DFU interface (mk2 では interface 4) 経由の UAC2 class 要求で
モニター・ミキサーなどの制御機能のみを扱います。alsa-ucm-conf の
`Audient-iD14-0008.conf` は mk2 用です。

2026-09-24 に mk2 で descriptor と GET 方向の要求を観測しました。SET (書込み) と
Linux 上の動作はまだ未照合です。Ubuntu での read-only 照合には
`tools/verify-linux.sh` を使ってください (書込みは一切行いません)。

Audient 社とは一切関係のない非公式プロジェクトであり、利用は自己責任で
お願いします。
