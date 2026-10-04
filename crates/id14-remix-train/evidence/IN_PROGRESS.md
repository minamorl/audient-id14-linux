# Training status: BLOCKED

The implementation and ABI/export path are verified, but no release model is
claimed yet. The owner accepted the MUSDB18-HQ educational/non-commercial terms
on 2026-10-05 with the answer `MUSDB18-HQ に同意する`. The dataset, its stems,
and every trained model are restricted to private non-commercial use; the audio
must not be redistributed. MoisesDB is excluded.

## Blocking condition

The current worker's assigned write scope is this Linux worktree only. The task
requires writes to the Mac at `~/datasets/musdb18hq` and a dedicated Mac
worktree, so this worker cannot download, extract, install PyTorch, or start the
background job without violating its scope.

Read-only inspection on 2026-10-05 found 200 GiB free, no
`~/datasets/musdb18hq`, no installed PyTorch in Python 3.14.7, and no remix
training process. The Mac has `/usr/bin/caffeinate`, `curl`, `unzip`, and
`ditto`. The official archive is 22,656,664,047 bytes with MD5
`12d4f2ecd55245a4688754dd76363103`.

Once a Mac-scoped worker runs the commands in `README.md`, each variant resumes
from `runs/SIZE-qQ/latest.pt`; progress is in `runs/SIZE-qQ/train.jsonl`, and
131k-q2 runs first. The first evaluation time cannot be estimated until that
worker measures the first real steps on MPS or CPU.

The `/tmp` one-step checkpoint and ONNX used during implementation are synthetic
smoke artifacts, not trained release candidates and are not part of this tree.
