# Sequence training benchmark

Measured on the Linux worker CPU with one PyTorch thread. Every sample includes
forward and backward for the 131K model. Medians are from three measured runs
after one warm-up run.

Command:

```sh
.venv/bin/python -m remix_train.benchmark_training \
  --size 131k --batches 1 2 4 8 --frames 32 64 --iterations 3 --threads 1
```

| batch | frames | old frame loop (s) | sequence (s) | speedup |
|---:|---:|---:|---:|---:|
| 1 | 32 | 0.108021 | 0.089122 | 1.212x |
| 1 | 64 | 0.216906 | 0.181348 | 1.196x |
| 2 | 32 | 0.222660 | 0.185981 | 1.197x |
| 2 | 64 | 0.474223 | 0.431641 | 1.099x |
| 4 | 32 | 0.374183 | 0.429039 | 0.872x |
| 4 | 64 | 0.760002 | 0.991461 | 0.767x |
| 8 | 32 | 0.684624 | 1.024808 | 0.668x |
| 8 | 64 | 1.377293 | 2.165129 | 0.636x |

This CPU result reflects cache/working-set behavior, not MPS kernel launch
behavior. CPU fallback should use batch 2 and 32 frames. MPS should first test
the existing batch 8 and 64-frame context because the optimization specifically
removes 64 serial GPU submissions while preserving the intended context length.
