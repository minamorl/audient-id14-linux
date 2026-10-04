# Playback runtime verification

Synthetic ONNX fixtures through the actual release .so; no hardware listening/xrun claim.

## Correction transfer, complete .so path

| Hz | Before dB | After dB |
|---:|---:|---:|
| 300 | -102.768080 | -82.366823 |
| 500 | -72.388996 | -0.077728 |
| 700 | -43.769809 | -0.044031 |
| 1000 | -10.861282 | 0.028480 |
| 1500 | -0.055427 | 0.064706 |

The narrower filter changes SR-inclusive processing delay from 96 ms to 100 ms.

## Limits and measured distinctions

- The isolated-frequency stereo gate is 1e-5 in complex L/R ratio; measured maximum is about 3.33e-8.
- The additional mixed-tone probe retains a maximum error of about 2.54e-5. Its explicit gate is 1e-4. Mixed-tone output is not claimed to have mathematically zero ratio error.
- The original 1e-5 mixed-tone gate failed on voice_bins. The raw discrepancy was retained and isolated-frequency verification was added; production DSP was not changed to hide the discrepancy.
- Callback times include ctypes call overhead and separately report calling-thread CPU time and wall time. No compiler or workspace tests ran concurrently with these timings.
- Peak verification uses separate 32x interpolation, including 17/19/21/23 kHz tones and already-hot input. This is fixture coverage, not an arbitrary-input proof.
- MemoryDenyWriteExecute=yes was applied by systemd-run to the second actual .so test run; the process exited 0.
- Model lookahead Q=0..3 is supported. A larger value is unsupported within the selected 100 ms budget and falls back visibly to neutral.
- Trained model delivery, CLI/bar/graph configuration and 30-minute iD14 hardware verification are outside this lane.

Full raw output: so-output.txt, mdwx-output.txt, workspace-test.log, release-build.txt. Artifact identity: artifact.json.

Timestamp convention: the host's default `ls` display uses JST (+09:00);
artifact.json stores the report-assembly timestamp in UTC. Thus 2026-10-05
08:26 JST and 2026-10-04 23:26 UTC describe the same report-assembly time.
