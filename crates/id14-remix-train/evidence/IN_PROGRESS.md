# Training status: IN_PROGRESS

The implementation and ABI/export path are verified, but no release model is
claimed yet. The six required trained checkpoints and MUSDB18-HQ evaluation are
blocked on owner-supplied corpora. The current worker's assigned write scope is
this worktree only, so it did not create a dataset or background job in the
separate Mac clone.

Required owner action:

1. Review and accept the MUSDB18-HQ educational/non-commercial license agreement
   on the official Zenodo record, then place the extracted corpus on the Mac.
2. Download MoisesDB from the official Music AI page for private non-commercial
   research and verify SHA-256
   `4cde33ce416ac7c868cffcb60eb31f5c741ab7ae5601cbb9d99ed498b72c48c1`.
3. Start the matrix from this directory as documented in `README.md`. Each
   variant resumes from `runs/SIZE-qQ/latest.pt`; `train.jsonl` is the progress
   log. Do not use `--fresh` when resuming.

The `/tmp` one-step checkpoint and ONNX used during implementation are synthetic
smoke artifacts, not trained release candidates and are not part of this tree.
