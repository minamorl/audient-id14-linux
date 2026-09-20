# Audient iD14 / iD14 MKII — USB control protocol (inferred)

> **UNOFFICIAL. NOT VERIFIED AGAINST HARDWARE.**
>
> This project is not affiliated with, endorsed by, or supported by Audient Ltd.
> Every value on this page is a **static-analysis inference** recovered from the
> official macOS application **iD v4.1.12 (2021)** (x86_64 slice). No device was
> attached while this document was written, no USB traffic was captured, and no
> request was ever sent. Treat every row as a hypothesis until the
> *hardware-confirmed* column is filled in from a real capture.
>
> Evidence status for this whole document: `static_analysis_inferred_unverified`.
> Nothing here is `hardware_confirmed`.

See [`provenance.md`](provenance.md) for where each value came from (distribution
URL, version, SHA-256, function name + address) and for the MixiD cross-check.

## How to read the tables

Each fact row has two evidence columns:

| column | meaning |
|---|---|
| **inferred (static)** | Value read from constants / structure copies in the official app binary. Cited as `Class::method @ address` (macOS x86_64 virtual address, pre-ASLR). |
| **hardware-confirmed** | Value observed in a USB capture of the official app talking to a real iD14, or confirmed by a successful request from this project. **Currently empty for every row.** |

A row whose hardware-confirmed cell reads `— (unverified)` MUST NOT be described
anywhere as "confirmed", "known", or "verified". When a capture is done, fill the
cell with the capture date and log reference; do not delete the inferred column.

---

## 1. Product table (VID / PID)

| model | field | value | inferred (static) | hardware-confirmed |
|---|---|---|---|---|
| iD14 (mk1) | idVendor | `0x2708` | `Id14ProductDefinition::getVID` @ `0x10001e220` returns constant `0x2708` | — (unverified) |
| iD14 (mk1) | idProduct | `0x0002` | `Id14ProductDefinition::getPID` @ `0x10001e210` returns constant `2` | — (unverified) |
| iD14 MKII (mk2) | idVendor | `0x2708` | `Id14mk2ProductDefinition::getVID` @ `0x100007760` returns constant `0x2708` | — (unverified) |
| iD14 MKII (mk2) | idProduct | `0x0008` | `Id14mk2ProductDefinition::getPID` @ `0x100007750` returns constant `8` | — (unverified) |

Corroboration (still not hardware confirmation): the Windows driver INF in the
same release lists hardware IDs `VID_2708&PID_0002` and `VID_2708&PID_0008`, and
the independent MixiD project uses the same IDs (see `provenance.md` §2).

The project supports both variants; anything that differs between mk1 and mk2
must live in the product-definition table, not be hard-coded elsewhere.

---

## 2. Control-request format

### 2.1 Leading 16-bit header per request kind

The three request-building methods in `UacLib::UacUnitBase` each write a 16-bit
constant to the first two bytes of the request object before handing it to
`UacLib::UsbDeviceImpl::sendRequest` (`0x10010f400`), which copies the request's
fields into a transmit structure and dispatches through an indirect call.

| request kind | header (u16, host order) | bytes on the wire (LE) | inferred (static) | hardware-confirmed |
|---|---|---|---|---|
| GET | `0x01a1` | `a1 01` | `UacUnitBase::sendGetRequest` @ `0x100106280` stores `0x1a1` at offset 0 | — (unverified) |
| SET | `0x0121` | `21 01` | `UacUnitBase::sendSetRequest` @ `0x10010bce0` stores `0x121` at offset 0 | — (unverified) |
| GET_MEM | `0x03a1` | `a1 03` | `UacUnitBase::sendGetMemRequest` @ `0x10010be20` stores `0x3a1` at offset 0 | — (unverified) |

### 2.2 Interpretation as USB control-transfer fields

Reading the two header bytes as the first two fields of a standard USB SETUP
packet gives the following. **This mapping is an interpretation of a structure
copy, not an observed packet.**

| request kind | bmRequestType | bRequest | wValue | wIndex | payload | inferred (static) | hardware-confirmed |
|---|---|---|---|---|---|---|---|
| GET | `0xa1` (IN, Class, Interface) | `0x01` | *open* | *open* | *open* (direction: device → host) | header byte order from `sendGetRequest` @ `0x100106280`; field roles from USB spec layout | — (unverified) |
| SET | `0x21` (OUT, Class, Interface) | `0x01` | *open* | *open* | *open* (direction: host → device) | header byte order from `sendSetRequest` @ `0x10010bce0`; field roles from USB spec layout | — (unverified) |
| GET_MEM | `0xa1` (IN, Class, Interface) | `0x03` | *open* | *open* | *open* (direction: device → host) | header byte order from `sendGetMemRequest` @ `0x10010be20`; field roles from USB spec layout | — (unverified) |

Notes on the interpretation:

- `bRequest = 0x01` for GET/SET is consistent with a UAC-style `CUR` request and
  `0x03` with a memory / range-style read, but the app's own enum
  (`UacLib::UacDescriptor::RequestType`) has **not** been decoded, so these names
  are not asserted.
- The mangled signatures visible in the symbol table are
  `sendGetRequest(RequestType, char, char, unsigned char*, unsigned int)`,
  `sendSetRequest(RequestType, char, char, unsigned char*, unsigned int)`, and
  `sendGetMemRequest(short, unsigned char*, unsigned int)`. It is *plausible* that
  the two `char` arguments become the `wValue` halves (control selector / channel)
  and the `unsigned char*, unsigned int` pair is the payload — but optimisation
  may reorder or fold arguments, so **wValue / wIndex / payload semantics are
  recorded as OPEN** until the callers are traced or a capture exists.
- Which interface number goes into `wIndex` (audio-control vs a spare
  vendor/DFU interface) is likewise **open**.

### 2.3 Open items (not yet recovered)

| item | status |
|---|---|
| `wValue` encoding (selector / channel) per control | open — not recovered |
| `wIndex` encoding (unit ID / interface) per control | open — not recovered |
| payload width and encoding per control | open — not recovered |
| meaning of `RequestType` enum values | open — not recovered |
| GET_MEM address space / semantics | open — not recovered |
| polling / notification of state changes | open — not recovered |

---

## 3. Extension Unit table (`getExtensionCode`)

Each Audient extension-unit class in `UacLib` overrides `getExtensionCode()` and
returns a small constant:

| class | getExtensionCode() | inferred (static) | hardware-confirmed |
|---|---|---|---|
| `UacXUAudientRoutingMatrix` | `0` | `getExtensionCode` @ `0x100106fd0` returns `0` | — (unverified) |
| `UacXUAudientMonitorController` | `1` | `getExtensionCode` @ `0x100107030` returns `1` | — (unverified) |
| `UacXUAudientMonoMixController` | `2` | `getExtensionCode` @ `0x100107000` returns `2` | — (unverified) |
| `UacXUAudientBlendMixer` | `0` | `getExtensionCode` @ `0x100107060` returns `0` | — (unverified) |
| `UacXUAudientInputControl` | `0` | `getExtensionCode` @ `0x100107090` returns `0` | — (unverified) |
| `UacXUAudientOutputControl` | `0` | `getExtensionCode` @ `0x1001070c0` returns `0` | — (unverified) |

> **These return values are NOT assumed to be USB Unit IDs or on-wire command
> values.** They are the return values of a C++ virtual method inside the app.
> How (or whether) they are mapped onto `wIndex`, a unit ID, or a command byte is
> unknown. Three distinct classes returning `0` is itself a hint that the value is
> an internal discriminator rather than a wire identifier.

---

## 4. Scope reminder

- Audio streaming is handled by the kernel `snd-usb-audio` driver; this project
  does not implement a streaming driver.
- No Audient binary, extracted tree, raw disassembly, or symbol dump is
  redistributed in this repository. Only transcribed constants and their
  provenance (function name + address) appear here.
