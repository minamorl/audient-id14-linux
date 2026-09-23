#!/usr/bin/env bash
# verify-linux.sh — read-only evidence collection for audient-id14-linux on Linux.
#
# Runs five checks against an attached Audient iD14 MKII (2708:0008; falls back
# to the mk1, 2708:0002) and prints PASS / FAIL for each:
#
#   1. lsusb descriptor dump (and presence of the DFU interface, class 0xFE)
#   2. snd-usb-audio is the driver bound to interfaces 0, 1 and 2 (sysfs)
#   3. id14ctl list
#   4. id14ctl info
#   5. id14ctl dump
#
# READ-ONLY. This script only reads: lsusb output, sysfs, /etc/os-release, and
# the output of the read-only id14ctl subcommands list / info / dump. It never
# invokes any other id14ctl subcommand, never passes a write-enabling flag,
# never loads / unloads / binds / unbinds a driver, and never writes to sysfs
# or to a device node. It needs no root.
#
# id14ctl is looked up in $ID14CTL, then PATH, then target/release and
# target/debug of this repository.
#
# Exit status: 0 if all five checks pass, 1 otherwise.

set -u

readonly VID="2708"
readonly PID_MK2="0008"
readonly PID_MK1="0002"
readonly SYSFS_USB="/sys/bus/usb/devices"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

results=()
failed=0

section() {
    printf '\n==== %s ====\n' "$1"
}

record() {
    # record <PASS|FAIL> <check name> <detail>
    results+=("$1  $2 — $3")
    if [ "$1" != "PASS" ]; then
        failed=1
    fi
    printf -- '--> %s: %s (%s)\n' "$1" "$2" "$3"
}

# --- environment (context only, not a check) ---------------------------------
section "environment"
printf 'date:   %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'kernel: %s\n' "$(uname -r)"
if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    printf 'os:     %s\n' "$(. /etc/os-release && printf '%s' "${PRETTY_NAME:-unknown}")"
fi
if [ -d /sys/module/snd_usb_audio ]; then
    printf 'snd_usb_audio module: loaded\n'
else
    printf 'snd_usb_audio module: not loaded\n'
fi

# --- locate the device --------------------------------------------------------
pid=""
model=""
if command -v lsusb >/dev/null 2>&1; then
    if lsusb -d "${VID}:${PID_MK2}" >/dev/null 2>&1; then
        pid="${PID_MK2}"
        model="iD14 MKII (mk2)"
    elif lsusb -d "${VID}:${PID_MK1}" >/dev/null 2>&1; then
        pid="${PID_MK1}"
        model="iD14 (mk1) — static-analysis inferred only, hardware-unverified"
    fi
fi
if [ -n "${pid}" ]; then
    printf 'device: %s:%s %s\n' "${VID}" "${pid}" "${model}"
else
    printf 'device: no %s:%s or %s:%s found by lsusb\n' "${VID}" "${PID_MK2}" "${VID}" "${PID_MK1}"
fi

# --- check 1: lsusb descriptor -------------------------------------------------
section "1. lsusb descriptor"
if ! command -v lsusb >/dev/null 2>&1; then
    record FAIL "lsusb descriptor" "lsusb not installed (apt install usbutils)"
elif [ -z "${pid}" ]; then
    record FAIL "lsusb descriptor" "device not found"
else
    # lsusb -v reads descriptors; without root some string fields may be missing.
    descriptor="$(lsusb -v -d "${VID}:${pid}" 2>&1)"
    printf '%s\n' "${descriptor}"
    if printf '%s\n' "${descriptor}" | grep -Eq 'bInterfaceClass[[:space:]]+254'; then
        record PASS "lsusb descriptor" "${VID}:${pid} dumped; DFU interface (class 0xFE) present"
    else
        record FAIL "lsusb descriptor" "${VID}:${pid} dumped, but no DFU interface (class 0xFE) found"
    fi
fi

# --- check 2: snd-usb-audio holds interfaces 0-2 --------------------------------
section "2. snd-usb-audio holds interfaces 0-2"
dev_dir=""
if [ -n "${pid}" ] && [ -d "${SYSFS_USB}" ]; then
    for d in "${SYSFS_USB}"/*; do
        [ -r "${d}/idVendor" ] && [ -r "${d}/idProduct" ] || continue
        if [ "$(cat "${d}/idVendor")" = "${VID}" ] && [ "$(cat "${d}/idProduct")" = "${pid}" ]; then
            dev_dir="${d}"
            break
        fi
    done
fi
if [ -z "${dev_dir}" ]; then
    record FAIL "snd-usb-audio holds interfaces 0-2" "device not found in ${SYSFS_USB}"
else
    dev_name="$(basename "${dev_dir}")"
    printf 'sysfs device: %s\n' "${dev_dir}"
    held=0
    for n in 0 1 2; do
        driver="none"
        for intf in "${SYSFS_USB}/${dev_name}":*."${n}"; do
            [ -e "${intf}" ] || continue
            if [ -L "${intf}/driver" ]; then
                driver="$(basename "$(readlink "${intf}/driver")")"
            fi
        done
        printf 'interface %s: driver=%s\n' "${n}" "${driver}"
        if [ "${driver}" = "snd-usb-audio" ]; then
            held=$((held + 1))
        fi
    done
    # Other interfaces are shown for context only (the DFU interface is expected
    # to be free of snd-usb-audio).
    for intf in "${SYSFS_USB}/${dev_name}":*.*; do
        [ -e "${intf}" ] || continue
        n="${intf##*.}"
        case "${n}" in 0 | 1 | 2) continue ;; esac
        driver="none"
        if [ -L "${intf}/driver" ]; then
            driver="$(basename "$(readlink "${intf}/driver")")"
        fi
        class="$(cat "${intf}/bInterfaceClass" 2>/dev/null || printf '?')"
        printf 'interface %s (class 0x%s): driver=%s\n' "${n}" "${class}" "${driver}"
    done
    if [ "${held}" -eq 3 ]; then
        record PASS "snd-usb-audio holds interfaces 0-2" "all three bound to snd-usb-audio"
    else
        record FAIL "snd-usb-audio holds interfaces 0-2" "${held}/3 bound to snd-usb-audio"
    fi
fi

# --- checks 3-5: id14ctl read-only subcommands ----------------------------------
ctl=""
if [ -n "${ID14CTL:-}" ] && [ -x "${ID14CTL}" ]; then
    ctl="${ID14CTL}"
elif command -v id14ctl >/dev/null 2>&1; then
    ctl="$(command -v id14ctl)"
elif [ -x "${repo_root}/target/release/id14ctl" ]; then
    ctl="${repo_root}/target/release/id14ctl"
elif [ -x "${repo_root}/target/debug/id14ctl" ]; then
    ctl="${repo_root}/target/debug/id14ctl"
fi

run_ctl() {
    # run_ctl <subcommand> — only the read-only subcommands are accepted here.
    local sub="$1"
    case "${sub}" in
        list | info | dump) ;;
        *)
            printf 'refusing non-read-only subcommand: %s\n' "${sub}" >&2
            exit 2
            ;;
    esac
    section "id14ctl ${sub}"
    if [ -z "${ctl}" ]; then
        record FAIL "id14ctl ${sub}" "id14ctl not found (set ID14CTL or run cargo build --release)"
        return
    fi
    printf '$ %s %s\n' "${ctl}" "${sub}"
    local out status
    out="$("${ctl}" "${sub}" 2>&1)"
    status=$?
    printf '%s\n' "${out}"
    if [ "${status}" -eq 0 ]; then
        record PASS "id14ctl ${sub}" "exit 0"
    else
        record FAIL "id14ctl ${sub}" "exit ${status}"
    fi
}

run_ctl list
run_ctl info
run_ctl dump

# --- summary ---------------------------------------------------------------------
section "summary"
for r in "${results[@]}"; do
    printf '%s\n' "${r}"
done
exit "${failed}"
