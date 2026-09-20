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
  identification, read-only state dump, and a `--dry-run` mode that prints the
  bytes it would send without touching the USB bus).

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

Early scaffolding. The protocol library and CLI are stubs; see the roadmap in
the documentation below.

Nothing in this project has been verified against real hardware yet — for
either model. When a real USB capture is done it will be taken on the owner's
mk2; mk1 stays inferred-only until someone with an mk1 unit contributes a
capture.

## Linux audio coexistence

This tool does **control only** (mk2-based description; the same intent
applies to mk1, unverified):

- Audio streaming stays with the kernel's `snd-usb-audio` driver. The tool
  **coexists** with it: it does **not** `rmmod` / unload `snd-usb-audio`, and it
  does **not** claim the USB audio interface exclusively.
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

## udev rules

`udev/99-audient-id14.rules` grants non-root access to both iD14 variants;
the mk2 (`2708:0008`) rules come first, then mk1 (`2708:0002`).
Copy it to `/etc/udev/rules.d/` and reload udev. The access mechanism
(`plugdev` group vs. `uaccess` tag) is provisional; see the comments in the
file.

## Documentation

- [`docs/protocol.md`](docs/protocol.md) — product definition table, control
  request format, extension-unit mapping, with "inferred" vs. "hardware
  confirmed" evidence kept in separate columns.
- [`docs/provenance.md`](docs/provenance.md) — where the protocol information
  came from (versions, hashes, referenced symbols) and cross-check results.

These documents are populated in a later step; the links may not resolve yet.

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
オーディオインターフェースを占有しません)、本プロジェクトは
モニター・ミキサーなどの制御機能のみを扱います。alsa-ucm-conf の
`Audient-iD14-0008.conf` は mk2 用です。

Audient 社とは一切関係のない非公式プロジェクトであり、利用は自己責任で
お願いします。
