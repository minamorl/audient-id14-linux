"""Train one size/lookahead variant on continuous contexts."""

from __future__ import annotations

import argparse
import json
import os
import random
import signal
import time
from pathlib import Path

import numpy as np
import torch

from .data import SequenceSampler, discover_moises, discover_musdb, discover_slakh, write_manifest
from .model import RemixNet


def synchronize(device: torch.device) -> None:
    if device.type == "mps":
        torch.mps.synchronize()
    elif device.type == "cuda":
        torch.cuda.synchronize(device)


def audio_cache_dir(args: argparse.Namespace) -> Path | None:
    if args.no_audio_cache:
        return None
    if args.cache_dir is not None:
        return args.cache_dir
    if args.musdb:
        root = args.musdb[0]
        return root.with_name(f"{root.name}-48k-cache")
    return None


def remix_loss(predicted, mixture, sources, gains):
    coefficient = ((gains - 1.0).unsqueeze(-1) * predicted).sum(dim=1)
    estimate = mixture * (1.0 + coefficient[:, None, :, None])
    reference = mixture + ((gains - 1.0)[:, :, None, None, None] * sources).sum(dim=1)
    error = (estimate - reference).square()
    scale = reference.square().flatten(1).mean(dim=1).clamp_min(1e-7)
    scale = scale.view(-1, *([1] * (error.ndim - 1)))
    return (error / scale).mean()


def pack_real_sequence(sequence):
    """Pack `[B,2,T,F,2]` real/imag as ABI frames `[B,T,4,F]`."""
    return torch.stack(
        (
            sequence[:, 0, :, :, 0],
            sequence[:, 0, :, :, 1],
            sequence[:, 1, :, :, 0],
            sequence[:, 1, :, :, 1],
        ),
        dim=2,
    )


def aligned_training_tensors(masks, mixture, sources, lookahead):
    frames = masks.shape[1] - lookahead
    aligned_masks = masks[:, lookahead:]
    aligned_mixture = mixture[:, :, :frames].permute(0, 2, 1, 3, 4)
    aligned_sources = sources[:, :, :, :frames].permute(0, 3, 1, 2, 4, 5)
    return aligned_masks, aligned_mixture, aligned_sources


def sampled_frame_sdr(masks, mixture, sources):
    estimate = mixture.unsqueeze(2) * masks.unsqueeze(3).unsqueeze(-1)
    signal = sources.square().sum(dim=(3, 4, 5))
    error = (sources - estimate).square().sum(dim=(3, 4, 5))
    values = 10.0 * torch.log10(signal / error.clamp_min(1e-12))
    result = {}
    for index, name in enumerate(("vocals", "drums", "bass", "other")):
        stem = values[:, :, index]
        valid = torch.isfinite(stem) & (signal[:, :, index] > 1e-8)
        selected = stem[valid]
        result[name] = {
            "frames": int(selected.numel()),
            "mean_db": float(selected.mean()) if selected.numel() else None,
            "median_db": float(selected.median()) if selected.numel() else None,
        }
    return result


def train(args: argparse.Namespace) -> None:
    random.seed(args.seed)
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)
    device = torch.device(args.device)
    tracks = []
    for root in args.moises:
        tracks += discover_moises(root)
    for root in args.slakh:
        tracks += [track for track in discover_slakh(root) if track.split == "train"]
    # Owner consent for private educational/non-commercial MUSDB18-HQ use was
    # recorded on 2026-10-05; only its train split enters optimization.
    for root in args.musdb:
        tracks += [track for track in discover_musdb(root) if track.split == "train"]
    run = args.output / f"{args.size}-q{args.lookahead}"
    run.mkdir(parents=True, exist_ok=True)
    write_manifest(run / "manifest.json", tracks)
    log = (run / "train.jsonl").open("a", buffering=1)
    cache_dir = audio_cache_dir(args)
    sampler = SequenceSampler(tracks, args.frames, args.seed, cache_dir)
    if sampler.cache_stats is not None:
        cache_event = {"event": "audio_cache", **sampler.cache_stats}
        print(json.dumps(cache_event), flush=True)
        print(json.dumps(cache_event), file=log)
    monitor_batch = None
    if args.artifact_every > 0:
        monitor_sampler = SequenceSampler(tracks, args.frames, args.seed + 1, cache_dir)
        monitor_batch = monitor_sampler.sample(min(args.batch, 2))
    model = RemixNet(args.size).to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, args.steps)
    start_step = 0
    checkpoint = run / "latest.pt"
    if checkpoint.is_file() and not args.fresh:
        saved = torch.load(checkpoint, map_location=device, weights_only=True)
        model.load_state_dict(saved["model"])
        optimizer.load_state_dict(saved["optimizer"])
        scheduler.load_state_dict(saved["scheduler"])
        start_step = int(saved["step"])
    stop_requested = False

    def request_stop(_signum, _frame):
        nonlocal stop_requested
        stop_requested = True

    signal.signal(signal.SIGTERM, request_stop)
    signal.signal(signal.SIGINT, request_stop)

    def save_checkpoint(step: int) -> None:
        temporary = run / "latest.pt.tmp"
        torch.save(
            {
                "model": model.state_dict(),
                "optimizer": optimizer.state_dict(),
                "scheduler": scheduler.state_dict(),
                "step": step,
                "size": args.size,
                "lookahead": args.lookahead,
            },
            temporary,
        )
        os.replace(temporary, checkpoint)

    def write_artifacts(step: int, row: dict) -> None:
        from .contract import check
        from .export import export

        assert monitor_batch is not None
        monitor_mixture = torch.view_as_real(monitor_batch[0]).to(device)
        monitor_sources = torch.view_as_real(monitor_batch[1]).to(device)
        monitor_state = torch.zeros(
            monitor_mixture.shape[0], model.state_size, device=device
        )
        with torch.no_grad():
            monitor_masks, _ = model.forward_sequence(
                pack_real_sequence(monitor_mixture), monitor_state
            )
            monitor_masks, monitor_mixture, monitor_sources = aligned_training_tensors(
                monitor_masks, monitor_mixture, monitor_sources, args.lookahead
            )
        onnx_path = run / f"remix-{step}.onnx"
        evaluation_path = run / f"evaluation-{step}.json"
        export(checkpoint, onnx_path, args.size, args.lookahead)
        report = {
            "step": step,
            "model": str(onnx_path),
            "split": "fixed training-corpus monitor batch; not final MUSDB18-HQ test",
            "monitor_seed": args.seed + 1,
            "monitor_batch": monitor_mixture.shape[0],
            "training": row,
            "frame_sdr": sampled_frame_sdr(
                monitor_masks, monitor_mixture, monitor_sources
            ),
            "contract": check(onnx_path),
        }
        temporary = evaluation_path.with_suffix(".json.tmp")
        temporary.write_text(json.dumps(report, indent=2) + "\n")
        os.replace(temporary, evaluation_path)
        event = {
            "event": "artifacts",
            "step": step,
            "onnx": str(onnx_path),
            "evaluation": str(evaluation_path),
        }
        print(json.dumps(event), flush=True)
        print(json.dumps(event), file=log)

    for step in range(start_step + 1, args.steps + 1):
        before = time.perf_counter()
        mixture, sources = sampler.sample(args.batch)
        sampled = time.perf_counter()
        # MPS does not support complex tensors. Keep an explicit final
        # real/imag axis so the same training loop runs on Apple Silicon.
        mixture = torch.view_as_real(mixture).to(device)
        sources = torch.view_as_real(sources).to(device)
        state = torch.zeros(args.batch, model.state_size, device=device)
        synchronize(device)
        transferred = time.perf_counter()
        masks, _ = model.forward_sequence(pack_real_sequence(mixture), state)
        aligned_masks, aligned_mixture, aligned_sources = aligned_training_tensors(
            masks, mixture, sources, args.lookahead
        )
        examples = args.batch * aligned_masks.shape[1]
        db = torch.empty(examples, 4, device=device).uniform_(-6.0, 6.0)
        gains = torch.pow(10.0, db / 20.0)
        loss = remix_loss(
            aligned_masks.reshape(examples, 4, 513),
            aligned_mixture.reshape(examples, 2, 513, 2),
            aligned_sources.reshape(examples, 4, 2, 513, 2),
            gains,
        )
        synchronize(device)
        forwarded = time.perf_counter()
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        synchronize(device)
        backwarded = time.perf_counter()
        optimizer.step()
        scheduler.step()
        synchronize(device)
        optimized = time.perf_counter()
        row = {
            "step": step,
            "loss": float(loss.detach()),
            "lr": scheduler.get_last_lr()[0],
            "seconds": optimized - before,
            "sample_seconds": sampled - before,
            "transfer_seconds": transferred - sampled,
            "forward_seconds": forwarded - transferred,
            "backward_seconds": backwarded - forwarded,
            "optimizer_seconds": optimized - backwarded,
            "size": args.size,
            "lookahead": args.lookahead,
            "parameters": model.parameter_count(),
        }
        print(json.dumps(row), flush=True)
        print(json.dumps(row), file=log)
        artifact_due = args.artifact_every > 0 and (
            step % args.artifact_every == 0 or step == args.steps
        )
        if step % args.save_every == 0 or step == args.steps or stop_requested or artifact_due:
            save_checkpoint(step)
        if artifact_due:
            write_artifacts(step, row)
        if stop_requested:
            stopped = {"event": "stopped", "step": step, "checkpoint": str(checkpoint)}
            print(json.dumps(stopped), flush=True)
            print(json.dumps(stopped), file=log)
            raise SystemExit(143)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--moises", type=Path, action="append", default=[])
    result.add_argument("--slakh", type=Path, action="append", default=[])
    result.add_argument("--musdb", type=Path, action="append", default=[])
    result.add_argument("--output", type=Path, default=Path("runs"))
    cache = result.add_mutually_exclusive_group()
    cache.add_argument("--cache-dir", type=Path)
    cache.add_argument("--no-audio-cache", action="store_true")
    result.add_argument("--size", choices=("131k", "444k"), required=True)
    result.add_argument("--lookahead", type=int, choices=(0, 2, 4), required=True)
    result.add_argument("--frames", type=int, default=64)
    result.add_argument("--batch", type=int, default=8)
    result.add_argument("--steps", type=int, default=20_000)
    result.add_argument("--save-every", type=int, default=100)
    result.add_argument("--artifact-every", type=int, default=2000)
    result.add_argument("--lr", type=float, default=3e-4)
    result.add_argument("--seed", type=int, default=1407)
    result.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    result.add_argument("--fresh", action="store_true")
    return result


def main() -> None:
    train(parser().parse_args())


if __name__ == "__main__":
    main()
