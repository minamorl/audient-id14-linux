# Provenance — where every value in `protocol.md` came from

This file records the exact upstream artefacts, the functions and addresses
that were read, and the independent cross-check, so that any number in
[`protocol.md`](protocol.md) can be traced back to a specific byte in a
specific file. Nothing here is hardware-confirmed; see the banner in
`protocol.md`.

## 1. Sources

### 1.1 Official distribution (Audient)

| field | value |
|---|---|
| downloads page | `https://audient.com/products/audio-interfaces/id14/downloads/` |
| acquisition date | 2026-09-20 |
| version (app) | **4.1.12** (2021 release series) |
| version (Windows driver INF `DriverVer`) | `01/20/2021, 4.2.0.24152` — note this is a *different* version number from the app |
| latest-version status | **unknown** — the downloads page linked this 2021 build; whether a newer app / firmware exists was not checked |

| file | distribution_url | sha256 |
|---|---|---|
| `iD-v4.1.12a.exe` (Windows, NSIS installer) | `https://d9w4fhj63j193.cloudfront.net/2021/1.%20iD%20Driver/iD-v4.1.12a.exe` | `f83ee177a6bd27acb05bef73c298a26f0379dcb45809fe55d6a87bda65112c42` |
| `iD v4.1.12.dmg` (macOS, HFS+ DMG) | `https://d9w4fhj63j193.cloudfront.net/2021/1.%20iD%20Driver/iD%20v4.1.12.dmg` | `c3835aa2d773a11f68ea6d7d221be2bc65921c8189b4a47ebc8dcc90105b46ce` |

Notes:

- The SHA-256 values identify **the originals that were acquired**; they are
  not a statement about authenticity. The code-signing / notarisation chain was
  **not** verified.
- The acquired version (4.1.12, 2021) **may not be the latest**; agreement with
  current firmware is an open question.
- The Windows INF inside the `.exe` lists `VID_2708&PID_0002` and
  `VID_2708&PID_0008`. The Windows API DLL exports names such as
  `TUSBAUDIO_AudioControlRequestGet/Set` and `TUSBAUDIO_ClassVendorRequestIn/Out`,
  but the Windows call graph was **not** traced; all protocol values below come
  from the macOS binary.
- Neither installer, driver, nor app was executed. No firmware was written.

### 1.2 Analysed binary

| field | value |
|---|---|
| path inside the DMG | `iD.app/Contents/MacOS/iD` |
| format | Universal Mach-O (x86_64 + arm64) |
| slice used for all addresses | **x86_64** |
| address kind | virtual address as linked, before ASLR slide |

### 1.3 Function name + address → fact

| fact (see `protocol.md`) | function_name | address | what was read |
|---|---|---|---|
| mk1 idProduct = `0x0002` | `Id14ProductDefinition::getPID() const` | `0x10001e210` | returns constant `2` |
| mk1 idVendor = `0x2708` | `Id14ProductDefinition::getVID() const` | `0x10001e220` | returns constant `0x2708` |
| mk2 idProduct = `0x0008` | `Id14mk2ProductDefinition::getPID() const` | `0x100007750` | returns constant `8` |
| mk2 idVendor = `0x2708` | `Id14mk2ProductDefinition::getVID() const` | `0x100007760` | returns constant `0x2708` |
| GET header = `0x01a1` | `UacLib::UacUnitBase::sendGetRequest(UacLib::UacDescriptor::RequestType, char, char, unsigned char*, unsigned int)` | `0x100106280` | stores 16-bit `0x1a1` at offset 0 of the request object |
| SET header = `0x0121` | `UacLib::UacUnitBase::sendSetRequest(UacLib::UacDescriptor::RequestType, char, char, unsigned char*, unsigned int)` | `0x10010bce0` | stores 16-bit `0x121` at offset 0 of the request object |
| GET_MEM header = `0x03a1` | `UacLib::UacUnitBase::sendGetMemRequest(short, unsigned char*, unsigned int)` | `0x10010be20` | stores 16-bit `0x3a1` at offset 0 of the request object |
| request dispatch path | `UacLib::UsbDeviceImpl::sendRequest(UacLib::UsbRequest&)` | `0x10010f400` | copies request fields into a transmit structure, dispatches via indirect call |
| RoutingMatrix code = `0` | `UacLib::UacXUAudientRoutingMatrix::getExtensionCode()` | `0x100106fd0` | returns `0` |
| MonoMixController code = `2` | `UacLib::UacXUAudientMonoMixController::getExtensionCode()` | `0x100107000` | returns `2` |
| MonitorController code = `1` | `UacLib::UacXUAudientMonitorController::getExtensionCode()` | `0x100107030` | returns `1` |
| BlendMixer code = `0` | `UacLib::UacXUAudientBlendMixer::getExtensionCode()` | `0x100107060` | returns `0` |
| InputControl code = `0` | `UacLib::UacXUAudientInputControl::getExtensionCode()` | `0x100107090` | returns `0` |
| OutputControl code = `0` | `UacLib::UacXUAudientOutputControl::getExtensionCode()` | `0x1001070c0` | returns `0` |

Additional symbols located but **not** yet analysed (listed so future work can
cite them): `Id14RoutingDefinition::getDefaultRouting(int, char) const`
@ `0x10001df00`, `Id14mk2RoutingDefinition::getDefaultRouting(int, char) const`
@ `0x1000073b0`.

## 2. MixiD cross-check

| field | value |
|---|---|
| repository | `https://github.com/TheOnlyJoey/MixiD` |
| commit read | `1435b9bc20c03c386026c4fa43f65c549e87e1ee` |
| commit date | 2026-08-03 (author timezone +02:00) |
| how read | `git clone --depth 1`, files read: `device_properties.h`, `driver.h`, `README.md` |
| role in this project | **cross-check only** — MixiD is *not* a code source. Its README declares the MIT License but the referenced `LICENSE.md` file is absent at this commit; independently of that, this project is clean-room from the official app, so no MixiD code is copied here. |

### 2.1 Comparison against the static-analysis values

| fact | static analysis (official app 4.1.12) | MixiD @ `1435b9b` | result |
|---|---|---|---|
| idVendor | `0x2708` | `0x2708` (`driver.h` vendor check and `libusb_open_device_with_vid_pid` call) | **match** |
| iD14 (mk1) idProduct | `0x0002` | `0x0002` (`device_properties.h`, entry `iD14`) | **match** |
| iD14 MKII (mk2) idProduct | `0x0008` | `0x0008` (`device_properties.h`, entry `iD14 MKII`) | **match** |
| SET request `bmRequestType`, `bRequest` | `0x21`, `0x01` (from header `0x0121`) | every `libusb_control_transfer` in `driver.h` uses `0x21`, `0x1` | **match** (consistent with the interpretation in `protocol.md` §2.2) |
| GET request `bmRequestType`, `bRequest` | `0xa1`, `0x01` (from header `0x01a1`) | no IN-direction (`0xa1`) control transfer present; README states read-back is "still in progress" | **not comparable** — MixiD has no GET path at this commit |
| GET_MEM request `bmRequestType`, `bRequest` | `0xa1`, `0x03` (from header `0x03a1`) | not present | **not comparable** |
| Extension-unit codes (0 / 1 / 2) | `getExtensionCode` return values | MixiD has no corresponding concept; it addresses controls by literal `wValue` / `wIndex` constants | **not comparable** |

Observations recorded for later, *not* adopted as facts:

- MixiD populates `wValue` and `wIndex` with literal constants per control
  (e.g. `wIndex = <unit byte> << 8 | control_iface`). This is MixiD's
  reverse-engineered model, tested by that project's users on real hardware per
  its support list; it has **not** been reconciled with the official app's
  request builders and is therefore not transcribed into `protocol.md`.
- MixiD's `driver.h` sends control requests through a spare DFU/vendor
  interface so that `snd-usb-audio` keeps the audio interface. Its README
  describes exclusive access; the two descriptions differ and were not resolved
  here.

## 3. Method

1. Downloaded the two official installers from links on the Audient downloads
   page; recorded byte counts and SHA-256.
2. Extracted both with 7-Zip **without executing** anything.
3. Parsed the macOS Universal Mach-O symbol table (x86_64 slice), demangled C++
   names, and selected the product-definition, extension-unit and
   request-sending functions.
4. Disassembled only those selected functions and read the immediate constants
   and store instructions from which the values above are taken.
5. Cross-checked VID/PID and the SET request type/request bytes against MixiD
   at the pinned commit.

No Audient binaries, extracted trees, raw disassembly listings, or symbol dumps
are redistributed in this repository; the working analysis directory is kept
outside the repo and is git-ignored (`originals/`, `extracted/`, `*.dmg`,
`*.exe`, `*.sys`, `*.dll`). Only the transcribed constants and their
provenance (function name + address) are published here.
