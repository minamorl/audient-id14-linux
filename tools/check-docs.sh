#!/usr/bin/env bash
# check-docs.sh — device-free checks for the documentation and the read-only
# verification script. Run from anywhere; exits non-zero on the first class of
# violation found (all violations of that class are printed).
#
#   A. tools/verify-linux.sh contains no write operation and calls only the
#      read-only id14ctl subcommands list / info / dump, all three of them.
#   B. docs/protocol.md: in every table whose last column is
#      "hardware-confirmed", each data row's last cell is exactly one of
#        "— (unverified)"
#        "mk2 hardware-observed 2026-09-24 (descriptor|GET|descriptor, GET)"
#      so no row can be labelled "confirmed"; mk1, SET and GET_MEM rows are
#      "— (unverified)"; and the document-wide status stays inferred.

set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="${root}/tools/verify-linux.sh"
protocol="${root}/docs/protocol.md"
rc=0

fail() {
    printf 'FAIL: %s\n' "$1"
    rc=1
}

# --- A. verify script is read-only ---------------------------------------------
if grep -nEw 'volume|mute|request' "${script}"; then
    fail "verify script mentions a write subcommand (volume / mute / request)"
fi
if grep -nF -- '--enable-write' "${script}"; then
    fail "verify script mentions --enable-write"
fi
if grep -nEw 'sudo|rmmod|modprobe|insmod|unbind|bind|usbreset|usb_modeswitch|dd|tee' "${script}"; then
    fail "verify script contains a privileged / driver-changing / writing command"
fi
if grep -nE '>[[:space:]]*/(sys|proc)/|>[[:space:]]*/dev/(bus|snd|hidraw|usb)' "${script}"; then
    fail "verify script redirects output into sysfs / procfs / a device node"
fi
bad_calls="$(grep -nE '^[[:space:]]*run_ctl[[:space:]]' "${script}" | grep -vE 'run_ctl[[:space:]]+(list|info|dump)[[:space:]]*$')"
if [ -n "${bad_calls}" ]; then
    printf '%s\n' "${bad_calls}"
    fail "verify script calls id14ctl with a subcommand other than list / info / dump"
fi
# Literal "${ctl}" is intended: it is the text searched for, not an expansion.
# shellcheck disable=SC2016
direct_calls="$(grep -nF '"${ctl}" "' "${script}" | grep -vF '"${ctl}" "${sub}"')"
if [ -n "${direct_calls}" ]; then
    printf '%s\n' "${direct_calls}"
    fail "verify script invokes id14ctl outside run_ctl"
fi
for sub in list info dump; do
    grep -qE "^run_ctl ${sub}\$" "${script}" || fail "verify script does not run id14ctl ${sub}"
done
grep -qE 'lsusb -v -d' "${script}" || fail "verify script does not dump the lsusb descriptor"
grep -qF 'snd-usb-audio holds interfaces 0-2' "${script}" || fail "verify script lacks the snd-usb-audio interfaces 0-2 check"

# --- B. protocol.md per-row evidence -------------------------------------------
awk_out="$(awk '
    function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
    function cells(line, arr,    tmp, n) {
        tmp = line
        gsub(/\\\|/, "\001", tmp)          # escaped pipes inside a cell
        sub(/^[ \t]*\|/, "", tmp); sub(/\|[ \t]*$/, "", tmp)
        n = split(tmp, arr, "|")
        return n
    }
    /^\|/ {
        n = cells($0, c)
        last = trim(c[n])
        if (!in_table) {
            in_table = 1; header = 1
            evidence = (last == "hardware-confirmed")
            next
        }
        if (header) { header = 0; next }       # separator row
        if (!evidence) next
        rows++
        ok = (last == "— (unverified)" ||
              last ~ /^mk2 hardware-observed 2026-09-24 \((descriptor|GET|descriptor, GET)\)$/)
        if (!ok) { printf "line %d: bad evidence cell: %s\n", NR, last; bad = 1 }
        if (last ~ /[Cc]onfirmed/) { printf "line %d: row labelled confirmed\n", NR; bad = 1 }
        first = trim(c[1])
        if ((first ~ /mk1/ || first ~ /^SET/ || first ~ /^GET_MEM/) && last != "— (unverified)") {
            printf "line %d: %s must stay unverified\n", NR, first; bad = 1
        }
        next
    }
    { in_table = 0 }
    END {
        if (rows == 0) { print "no hardware-confirmed tables found"; bad = 1 }
        printf "checked %d evidence rows\n", rows
        exit bad
    }
' "${protocol}")"
awk_rc=$?
printf '%s\n' "${awk_out}"
[ "${awk_rc}" -eq 0 ] || fail "docs/protocol.md evidence cells"

# shellcheck disable=SC2016
grep -qF 'as a whole** stays `static_analysis_inferred_unverified`' "${protocol}" ||
    fail "docs/protocol.md lost the document-wide inferred status"
if grep -nE 'hardware_confirmed' "${protocol}" | grep -vE '\*\*not\*\*|not[[:space:]]'; then
    fail "docs/protocol.md claims hardware_confirmed without negation"
fi

if [ "${rc}" -eq 0 ]; then
    printf 'check-docs: OK\n'
fi
exit "${rc}"
