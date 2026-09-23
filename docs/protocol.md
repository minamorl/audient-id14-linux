# Audient iD14 / iD14 MKII — USB control protocol

> **UNOFFICIAL. MOSTLY INFERRED. ONLY PARTLY OBSERVED ON HARDWARE.**
>
> This project is not affiliated with, endorsed by, or supported by Audient Ltd.
> Most values on this page are **static-analysis inferences** recovered from the
> official macOS application **iD v4.1.12 (2021)** (x86_64 slice). On
> **2026-09-24** a subset of rows was **observed on the owner's iD14 MKII (mk2)**:
> the device descriptors, and **GET-direction** requests only (GET CUR /
> GET RANGE). **No SET request has been sent to any device, and nothing has
> been checked on Linux yet.**
>
> Evidence status for this document **as a whole** stays `static_analysis_inferred_unverified`.
> The protocol as a whole is **not** `hardware_confirmed`.
> Hardware evidence is recorded **per row**, in the *hardware-confirmed*
> column of each table, and only for the rows it covers.

> **Primary target: iD14 MKII (mk2, `2708:0008`).** The mk2 is the project
> owner's unit, and the *hardware-confirmed* column in every table below refers
> to the **mk2** only. The original **iD14 (mk1, `2708:0002`)** stays in the
> product table and is supported, but every mk1 value is **static-analysis
> inferred only and hardware-unverified**; the hardware-confirmed column is not
> filled for mk1 rows by this project.

See [`provenance.md`](provenance.md) for where each value came from (distribution
URL, version, SHA-256, function name + address, the 2026-09-24 mk2 observation)
and for the MixiD cross-check.

## How to read the tables

Evidence is recorded **per row**. Each fact row has two evidence columns:

| column | meaning |
|---|---|
| **inferred (static)** | Value read from constants / structure copies in the official app binary. Cited as `Class::method @ address` (macOS x86_64 virtual address, pre-ASLR). `—` means the static analysis has no entry for this row. |
| **hardware-confirmed** | Hardware evidence for this row on the owner's **mk2** (`2708:0008`). mk1 rows are never filled. |

The *hardware-confirmed* column uses exactly two cell forms:

| cell | meaning |
|---|---|
| `mk2 hardware-observed 2026-09-24 (…)` | Observed on the mk2 on 2026-09-24. The parenthesis says how: `descriptor` (read from the device's USB descriptors) or `GET` (a GET-direction class request answered by the device). Not observed on Linux. |
| `— (unverified)` | No hardware evidence. The value is a static-analysis inference (or an open item) and MUST NOT be described anywhere as "confirmed", "known", or "verified". |

When new evidence arrives, replace `— (unverified)` in that row only, with
the date and how it was observed; do not delete the inferred column, and do not
promote neighbouring rows.

---

## 1. Product table (VID / PID)

| model | field | value | inferred (static) | hardware-confirmed |
|---|---|---|---|---|
| iD14 MKII (mk2) | idVendor | `0x2708` | `Id14mk2ProductDefinition::getVID` @ `0x100007760` returns constant `0x2708` | mk2 hardware-observed 2026-09-24 (descriptor) |
| iD14 MKII (mk2) | idProduct | `0x0008` | `Id14mk2ProductDefinition::getPID` @ `0x100007750` returns constant `8` | mk2 hardware-observed 2026-09-24 (descriptor) |
| iD14 (mk1) | idVendor | `0x2708` | `Id14ProductDefinition::getVID` @ `0x10001e220` returns constant `0x2708` | — (unverified) |
| iD14 (mk1) | idProduct | `0x0002` | `Id14ProductDefinition::getPID` @ `0x10001e210` returns constant `2` | — (unverified) |

Corroboration for all four rows: the Windows driver INF in the same release
lists hardware IDs `VID_2708&PID_0002` and `VID_2708&PID_0008`, and the
independent MixiD project uses the same IDs (see `provenance.md` §3).

The project supports both variants; anything that differs between mk1 and mk2
lives in the product-definition table, not hard-coded elsewhere.

Primary target is the **mk2** (owner's unit): default target, CLI auto-detect
priority, udev rule order, and test fixtures all prefer mk2. The **mk1** rows
above are static-analysis inferred only and hardware-unverified, and will stay
that way until an mk1 unit is examined.

---

## 2. Control-request format

Every request is a USB control transfer: an 8-byte SETUP packet
(`bmRequestType`, `bRequest`, `wValue`, `wIndex`, `wLength`) followed by a
data stage (the payload). `id14ctl --dry-run` prints exactly these bytes —
the 8-byte setup packet and the payload — and performs no USB transfer.

### 2.1 Request kinds (`bmRequestType` / `bRequest`)

The three request-building methods in `UacLib::UacUnitBase` each write a 16-bit
constant to the first two bytes of the request object before handing it to
`UacLib::UsbDeviceImpl::sendRequest` (`0x10010f400`). Read as the first two
SETUP fields, the low byte is `bmRequestType` and the high byte is `bRequest`.
GET RANGE does not appear in the static-analysis table; it was observed on the
mk2.

| request kind | header (u16, host order) | bytes on the wire (LE) | bmRequestType | bRequest | inferred (static) | hardware-confirmed |
|---|---|---|---|---|---|---|
| GET (CUR) | `0x01a1` | `a1 01` | `0xa1` (IN, Class, Interface) | `0x01` | `UacUnitBase::sendGetRequest` @ `0x100106280` stores `0x1a1` at offset 0 | mk2 hardware-observed 2026-09-24 (GET) |
| GET RANGE | — | `a1 02` | `0xa1` (IN, Class, Interface) | `0x02` | — | mk2 hardware-observed 2026-09-24 (GET) |
| SET (CUR) | `0x0121` | `21 01` | `0x21` (OUT, Class, Interface) | `0x01` | `UacUnitBase::sendSetRequest` @ `0x10010bce0` stores `0x121` at offset 0 | — (unverified) |
| GET_MEM | `0x03a1` | `a1 03` | `0xa1` (IN, Class, Interface) | `0x03` | `UacUnitBase::sendGetMemRequest` @ `0x10010be20` stores `0x3a1` at offset 0 | — (unverified) |

- **SET has never been sent to a device.** Its header is a static-analysis
  inference only; that `21 01` is correct on the mk2 is unverified, and so is
  every SET on Linux.
- GET_MEM was not part of the 2026-09-24 observation and has no per-row
  hardware evidence; it follows the document-wide inferred status.

### 2.2 `wValue` / `wIndex` layout

| field | layout | inferred (static) | hardware-confirmed |
|---|---|---|---|
| `wValue` | `(control selector << 8) \| channel number` | — (the app's `sendGetRequest` / `sendSetRequest` take two `char` arguments; their mapping onto `wValue` was not traced) | mk2 hardware-observed 2026-09-24 (GET) |
| `wIndex` | `(entity ID << 8) \| control interface number` | — (not recovered from the app) | mk2 hardware-observed 2026-09-24 (GET) |

The layout was observed on GET requests. SET requests are built with the same
layout by this project, but no SET has been sent (see §2.1).

### 2.3 Request format per kind

The five request fields, per request kind:

| request kind | bmRequestType | bRequest | wValue | wIndex | payload | hardware-confirmed |
|---|---|---|---|---|---|---|
| GET CUR | `0xa1` | `0x01` | `(CS << 8) \| CN` | `(entity << 8) \| DFU interface number` | device → host, control-dependent length | mk2 hardware-observed 2026-09-24 (GET) |
| GET RANGE | `0xa1` | `0x02` | `(CS << 8) \| CN` | `(entity << 8) \| DFU interface number` | device → host, control-dependent length | mk2 hardware-observed 2026-09-24 (GET) |
| SET CUR | `0x21` | `0x01` | `(CS << 8) \| CN` | `(entity << 8) \| DFU interface number` | host → device, control-dependent length | — (unverified) |
| GET_MEM | `0xa1` | `0x03` | *open* | *open* | *open* (device → host) | — (unverified) |

`CS` = control selector, `CN` = channel number, `entity` = the ID of the unit
or terminal the control belongs to (read from the descriptor at run time, §4).

### 2.4 Open items

| item | status |
|---|---|
| payload width and meaning of the Audient Extension Unit **vendor** controls (routing / monitor controller / mono mix) | open — the response length is not self-describing: reading 64 bytes returns one byte followed by leftovers of a previous buffer |
| SET on the mk2, and any request on Linux | open — not yet checked |
| meaning of the app's `RequestType` enum values | open — not recovered |
| GET_MEM address space / semantics | open — not recovered |
| polling / notification of state changes | open — not recovered |

---

## 3. Control path (mk2)

How the control requests reach the device, and what is deliberately left
alone so that `snd-usb-audio` keeps working.

| fact | value | inferred (static) | hardware-confirmed |
|---|---|---|---|
| USB Audio Class version | UAC2 (audio-control interface `bInterfaceProtocol` = `0x20`) | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| HID interface (interface 3) | input-only mouse; no output / feature report | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| vendor-specific interface | none | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| DFU interface number | `4` (interface class `0xFE`) | — | mk2 hardware-observed 2026-09-24 (descriptor) |

The control path is **UAC2 class requests addressed through the DFU
interface**:

1. Find the interface with class `0xFE` (DFU) **in the device descriptor**.
2. Claim **only that interface**. Interfaces 0–2 (audio control / streaming,
   owned by `snd-usb-audio`) are **never claimed**, and **no kernel driver is
   detached**.
3. Put that interface number in the **low byte of `wIndex`**.
4. If no DFU interface is found, **stop with an error**. There is no fallback
   to interface 0.

The control path is not hidraw and not a vendor interface (the mk2 has no
vendor interface and its HID interface is input-only).

---

## 4. Entities and controls declared by the mk2

Entity IDs (`wIndex` high byte) are **read from the descriptor at run time**.
They are never derived from an extension code (§5). The mk2 values below are
what the mk2 descriptor declares; they are documented for reference and test
fixtures, not hard-coded as the source of truth.

| entity | declared controls | inferred (static) | hardware-confirmed |
|---|---|---|---|
| clock source | `SAM_FREQ` | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| clock selector | clock selector control | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| Feature Unit `0x0C` (output side of the mixer unit) | volume, channels 1–4; GET RANGE = −127 dB … 0 dB, step 1 dB | — | mk2 hardware-observed 2026-09-24 (descriptor, GET) |
| Feature Unit `0x0B` | per-channel controls | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| Mixer Unit `0x3C` | 16 inputs × 6 outputs | — | mk2 hardware-observed 2026-09-24 (descriptor) |
| mute | **no Feature Unit declares a mute control** | — | mk2 hardware-observed 2026-09-24 (descriptor) |

Consequences for `id14ctl`:

- `dump` reads (GET CUR, and GET RANGE where needed) only the standard
  controls the descriptor declares: clock source `SAM_FREQ`, clock selector,
  the declared Feature Unit controls, and the mixer unit. It sends no SET and
  does **not** read Extension Unit vendor controls (their width is open, §2.4).
- `volume` writes (SET CUR) the volume control of the Feature Unit on the
  mixer's output side (FU `0x0C` on the mk2). The value is given in dB; a value
  outside the device's GET RANGE is rejected without being sent. After the
  write the value is read back with GET CUR and shown. Gated behind
  `--enable-write`; SET is unverified on hardware (§2.1).
- `mute` fails, stating that the device declares no mute control. It does not
  substitute anything (e.g. minimum volume).

---

## 5. Extension Unit table (`getExtensionCode` ↔ `wExtensionCode`)

Each Audient extension-unit class in `UacLib` overrides `getExtensionCode()` and
returns a small constant. On the wire this value appears as the UAC2
**`wExtensionCode`** field of an Extension Unit (XU) descriptor. It is **not**
the unit ID: the unit ID is a separate descriptor field, and the mapping from
code to unit ID is whatever the descriptor says.

| class | getExtensionCode() | wire field | mk2 XU unit ID(s) carrying this `wExtensionCode` | inferred (static) | hardware-confirmed |
|---|---|---|---|---|---|
| `UacXUAudientMonitorController` | `1` | `wExtensionCode` = `0x0001` | `0x36` | `getExtensionCode` @ `0x100107030` returns `1` | mk2 hardware-observed 2026-09-24 (descriptor) |
| `UacXUAudientMonoMixController` | `2` | `wExtensionCode` = `0x0002` | `0x37` | `getExtensionCode` @ `0x100107000` returns `2` | mk2 hardware-observed 2026-09-24 (descriptor) |
| `UacXUAudientRoutingMatrix` | `0` | `wExtensionCode` = `0x0000` | undetermined (see below) | `getExtensionCode` @ `0x100106fd0` returns `0` | — (unverified) |
| `UacXUAudientBlendMixer` | `0` | `wExtensionCode` = `0x0000` | undetermined (see below) | `getExtensionCode` @ `0x100107060` returns `0` | — (unverified) |
| `UacXUAudientInputControl` | `0` | `wExtensionCode` = `0x0000` | undetermined (see below) | `getExtensionCode` @ `0x100107090` returns `0` | — (unverified) |
| `UacXUAudientOutputControl` | `0` | `wExtensionCode` = `0x0000` | undetermined (see below) | `getExtensionCode` @ `0x1001070c0` returns `0` | — (unverified) |

> **Extension codes are not unit IDs.** On the mk2, `wExtensionCode` `0x0000`
> appears on several XUs (`0x3E`, `0x32`, `0x34`, `0x33`); which of them is the
> RoutingMatrix (or any other code-0 class) is **not determined**. Do not
> derive a unit ID from an extension code — read the unit ID from the
> descriptor. The width and meaning of the vendor controls inside these XUs are
> open (§2.4), so no XU vendor control is read or written by this project yet.

---

## 6. Scope reminder

- Audio streaming is handled by the kernel `snd-usb-audio` driver; this project
  does not implement a streaming driver and does not unload or exclusively
  claim anything `snd-usb-audio` uses.
- No Audient binary, extracted tree, raw disassembly, or symbol dump is
  redistributed in this repository. Only transcribed constants and their
  provenance (function name + address) appear here.
