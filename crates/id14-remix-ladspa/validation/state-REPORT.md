# remix-state-v1 follow-up

Scope: audient-id14-realtime-sr.spec v0.13, remixing_overload_visibility,
and the caller's explicit remix-state-v1 seam. No existing target source outside
this worker's own crate was read or changed. No commit was made by this worker.

The publication worker owns all state-file I/O and cleanup. The inference worker
can stop independently. Audio only updates atomics. State/counter snapshots use
one packed atomic, so the published counter belongs to the published state.
Polling is 10 ms; brief changes coalesce into the latest state. The counter counts
entries into Overloaded, including entries between polls, and survives activate.
Heartbeat attempts are at least one second apart when state/counter is unchanged.

Actual shared-object evidence is state-so-output.txt:
- Active JSON includes the exact contract keys, model path, PID/instance and 3776 frames.
- Off updates the file; the measured unchanged-state heartbeat interval is 1008 ms.
- Stopping inference produces Overloaded and overloads=1 in the same file.
- Cleanup removes its own file while preserving another live instance's file.
- XDG unset, ENOTDIR and EACCES each produce zero differing audio bits versus the
  writable case, for both Enabled=0 and Enabled=1, after startup settles.
- These are synthetic fixture/host tests, not a PipeWire integration claim.

Commands and actual exit codes are in state-commands.txt. Unit and existing
workspace output is state-workspace-test.log. Release output is
state-release-build.log. The existing audio .so harness also ran with publication
enabled; raw output and measurements are state-regression-output.txt and
state-regression-results.json. Current binary identity is state-artifact.sha256;
the older artifact.json/REPORT.md describe the prior runtime build.

Tooling: cargo fmt was unavailable (exit 101: no such command `fmt`). Formatting
was performed with nixpkgs#rustfmt directly (exit 0), on the three edited Rust
implementation files only.

assumptions: polling interval, coalescing, overload-entry counting, persistent
instance identity across activate, and lossy display of non-UTF-8 model paths
are implementation choices. No additional specification contradiction or
undecidable point was encountered.

verdict: IMPLEMENTED
