# audient-id14-linux

Linux control tool for the **Audient iD14** (mk1, USB `2708:0002`) and
**iD14 MKII** (mk2, USB `2708:0008`) audio interfaces.

Audio streaming on Linux already works through the kernel's `snd-usb-audio`
driver. What is missing is a way to drive the device's *control* features
(monitor/mixer state, routing, etc.). This project aims to fill that gap with:

- `id14-protocol` — a pure, host-independent Rust library that builds and
  interprets iD14 control packets (unit-tested without hardware).
- `id14ctl` — a small command-line tool on top of it (device listing, product
  identification, read-only state dump, and a `--dry-run` mode that prints the
  bytes it would send without touching the USB bus).

The tool is designed to coexist with `snd-usb-audio`; it does not replace the
audio driver and does not require unloading it.

## Disclaimer

**This is an unofficial project. It is not affiliated with, endorsed by, or
supported by Audient in any way. "Audient" and "iD14" are trademarks of their
respective owner. Use this software at your own risk: sending control requests
to a USB audio interface may leave it in an unexpected state, and the authors
accept no liability for any damage or data loss.**

## Status

Early scaffolding. The protocol library and CLI are stubs; see the roadmap in
the documentation below.

## Building

```sh
cargo build --workspace
cargo test --workspace
```

`id14ctl` will require `libusb-1.0` development headers once its USB backend
lands (`sudo apt-get install libusb-1.0-0-dev pkg-config` on Debian/Ubuntu).

## udev rules

`udev/99-audient-id14.rules` grants non-root access to both iD14 variants.
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

Audient iD14 (初代 / MKII) を Linux から制御するための非公式ツールです。
音声のストリーミングは既存の `snd-usb-audio` に任せ、本プロジェクトは
モニター・ミキサーなどの制御機能のみを扱います。

Audient 社とは一切関係のない非公式プロジェクトであり、利用は自己責任で
お願いします。
