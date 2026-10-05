# Trained-model worker performance

Domain: audient-id14-realtime-sr.spec v0.13. Runtime/threading implementation is
free; preserve remix-v1 and the playback pins. Additional caller gate: inference
p99 <= 5 ms and Active at 256/48000-second callback intervals.

Model: caller-supplied /tmp/id14-remix-models/remix-2000.onnx, copied unchanged to
.build/remix-2000.onnx for measurements. 544901 bytes, SHA256
d22d567981f3d895d36a99a74d934462f79f03555ef74ec94ded855d4a5dd760.
The trained model is not added as a shipped fixture.

## Wall-clock measurements (milliseconds)

| Path | Samples | p50 | p99 | max |
|---|---:|---:|---:|---:|
| Original adapter, tract 0.22.1 | 300 | 14.764382 | 16.140950 | 16.557479 |
| Original runtime, execution-state reuse experiment | 300 | 14.774711 | 16.072460 | 16.244770 |
| Final adapter, ORT CPU 1 thread | 500 | 0.703789 | 0.860620 | 0.916229 |
| Final actual plugin worker, systemd MDWX=yes | 2911 | 0.714980 | 1.143805 | 1.579179 |

Original measurements: perf-before.txt. Final standalone/per-operator measurements:
perf-comparison.txt. Final actual plugin: perf-mdwx-realtime.txt. Separate phase
batches and operator timing hooks have different overhead/cache/scheduling, so
their percentiles must not be added together. No own compiler/test process ran
concurrently with timed inference/audio workloads.

The original plan is created once, not recreated per hop. Tract plan.run creates
an execution state per hop, but this is not the bottleneck: the original baseline
median state initialization is 0.00644 ms, input allocation/copy 0.001 ms, and
output copy/validation 0.00277 ms. Retaining that state did not meet the gate.

The final comparison's instrumented tract operator group means per hop are:
Im2col 13.791381 ms, Resize 1.363439 ms, OptMatMul 0.678592 ms,
matrix packing 0.015774 ms, GRU 0.047198 ms. These identify convolution input
rearrangement as the dominant observed cost, rather than GRU or adapter copies.

The old runtime startup phases in that comparison are file read 0.31079 ms,
protobuf decode 0.64742 ms, ONNX import 0.335269 ms, into_optimized 29.631492 ms,
and into_runnable 0.06501 ms. All occur once on model loading.

Final ORT startup: adapter load+probe 30.078562 ms; decode/validation 0.94431 ms,
native runtime initialization 8.715784 ms, session optimization/build plus input
tensor allocation 18.719669 ms, probe 1.180789 ms. Session construction/optimization
does not occur during inference. Persistent input tensors and explicit streaming
state are reused. Final per-hop phase p50/p99: input copy 0.00023/0.00080 ms,
session.run 0.69383/0.75008 ms, output copy/validation 0.00279/0.00379 ms.

## Actual shared-object verification

With MemoryDenyWriteExecute=yes applied by systemd, the real trained model ran
30 seconds at absolute 5.333333 ms deadlines: 5625/5625 measured blocks are
State=1, zero Overloaded. The preceding 187 warmup blocks are also State=1.
Actual interval median 5.333306 ms, p99 5.536502 ms; maximum scheduling lateness
1.448979 ms. Callback wall time p99 980.511637 us, maximum 1921.919 us.
Inference clocks and timing-ring writes occur solely on the inference worker;
the host reads the timing ring after joining it. The measured latency is still
3776 frames (4800 frames / 100 ms including the unchanged SR delay).

The existing .so contract host also ran: perf-so-output.txt/perf-so-results.json.
Missing/OFF/zero/in-place paths have zero differing bits and correlation delay
3776 frames. Worker-stop neutral settling is 64.645833 ms with envelope sample
step 0.000086857 and minimum amplitude 0.099999999 for the 0.1 fixture. At 299/300 Hz,
relative changes are -93.204058/-90.057554 dB. Isolated-bin L/R ratio maximum error
is 3.332904e-8; silent output has zero nonzero samples. Original mixed-tone and
finite true-peak reconstruction limitations remain documented in REPORT.md.

State publication recheck: perf-state-output.txt. Native-runtime absent, invalid
ELF and missing OrtGetApiBase checks use fresh processes and yield state 5 with
zero differing audio bits (perf-runtime-failure.txt, perf-invalid-library.txt,
perf-missing-symbol.txt). zero_state.onnx also verifies S=0 compatibility.

## Discovered differences and corrected verification

The added expectation that ORT and tract masks differ by <1e-4 failed, with a
0.06360893 maximum over 32 hops. Repeating with the old plugin's fresh execution
state per hop produced the same first-hop discrepancy. Raw failed runs remain
perf-comparison-initial.txt, perf-comparison-fresh-state.txt and
perf-parity-investigation.txt (exit 101). This was not solved by widening a gate.

An independent ONNX ReferenceEvaluator run on the identical first-hop input and
zero state yields maximum mask errors ORT=2.98023224e-7, tract=0.0636089444
(perf-onnx-reference.txt, exit 0). Thus the old runtime is not a suitable equality
oracle for this input. The final benchmark reports the ORT/tract difference as
information; the separate reference test retains its 1e-4 ORT tolerance. The
final exported comparison vectors are byte-identical to the independently
checked vectors (cmp exit 0). This is one input's reference coverage, not a proof
for every input; the exact operator responsible for the old numerical discrepancy
was not localized. It is separate from the measured Im2col performance cost.

The initial ort init_from error path hung when the native library was absent
(host check exit 1, direct adapter timeout exit 124; raw evidence retained).
The binding's error construction calls CreateStatus before native API bootstrap.
The final adapter uses libloading and checked OrtGetApiBase/GetApi bootstrap,
then ort::set_api, avoiding any ort error object before the API is available.
The library handle is owned for process lifetime so API pointers cannot dangle.

## Delivery and scope

Production inference is ONNX Runtime 1.27.1, ort 2.0.0-rc.12, CPU only, one
intra/inter thread, sequential graph execution, optimization level 3, no spinning.
Tract remains a protobuf validator and explicit benchmark reference only.
The build wrapper embeds the Nix native library path; ID14_ORT_LIBRARY overrides
it at startup. Deployment must retain the native runtime in its Nix closure.
No PipeWire configuration, trained-model distribution, external deployment or
commit was performed. This is offline real-time-equivalent execution, not an
iD14 hardware xrun/listening claim.

assumptions: runtime selection/options and diagnostic timing ring size 8192 are
implementation choices. The independent comparison tolerance 1e-4 is a diagnostic
choice, not a new domain pin. No unresolved pin decision or contradiction.

verdict: IMPLEMENTED
