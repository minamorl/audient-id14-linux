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


def remix_loss(predicted, mixture, sources, gains):
    coefficient = ((gains - 1.0).unsqueeze(-1) * predicted).sum(dim=1)
    estimate = mixture * (1.0 + coefficient[:, None, :, None])
    reference = mixture + ((gains - 1.0)[:, :, None, None, None] * sources).sum(dim=1)
    error = (estimate - reference).square()
    scale = reference.square().flatten(1).mean(dim=1).clamp_min(1e-7)
    scale = scale.view(-1, *([1] * (error.ndim - 1)))
    return (error / scale).mean()


def pack_real_x(frame):
    """Pack [B,2,F,2] real/imag without complex MPS tensors."""
    return torch.stack(
        (frame[:, 0, :, 0], frame[:, 0, :, 1], frame[:, 1, :, 0], frame[:, 1, :, 1]),
        dim=1,
    )


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
    sampler = SequenceSampler(tracks, args.frames, args.seed)
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
    log = (run / "train.jsonl").open("a", buffering=1)
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

    for step in range(start_step + 1, args.steps + 1):
        before = time.perf_counter()
        mixture, sources = sampler.sample(args.batch)
        # MPS does not support complex tensors. Keep an explicit final
        # real/imag axis so the same training loop runs on Apple Silicon.
        mixture = torch.view_as_real(mixture).to(device)
        sources = torch.view_as_real(sources).to(device)
        state = torch.zeros(args.batch, model.state_size, device=device)
        losses = []
        for frame in range(args.frames):
            mask, state = model(pack_real_x(mixture[:, :, frame]), state)
            if frame >= args.lookahead:
                target = frame - args.lookahead
                db = torch.empty(args.batch, 4, device=device).uniform_(-6.0, 6.0)
                gains = torch.pow(10.0, db / 20.0)
                losses.append(
                    remix_loss(mask, mixture[:, :, target], sources[:, :, :, target], gains)
                )
        loss = torch.stack(losses).mean()
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        optimizer.step()
        scheduler.step()
        row = {
            "step": step,
            "loss": float(loss.detach()),
            "lr": scheduler.get_last_lr()[0],
            "seconds": time.perf_counter() - before,
            "size": args.size,
            "lookahead": args.lookahead,
            "parameters": model.parameter_count(),
        }
        print(json.dumps(row), flush=True)
        print(json.dumps(row), file=log)
        if step % args.save_every == 0 or step == args.steps or stop_requested:
            save_checkpoint(step)
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
    result.add_argument("--size", choices=("131k", "444k"), required=True)
    result.add_argument("--lookahead", type=int, choices=(0, 2, 4), required=True)
    result.add_argument("--frames", type=int, default=64)
    result.add_argument("--batch", type=int, default=8)
    result.add_argument("--steps", type=int, default=20_000)
    result.add_argument("--save-every", type=int, default=100)
    result.add_argument("--lr", type=float, default=3e-4)
    result.add_argument("--seed", type=int, default=1407)
    result.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    result.add_argument("--fresh", action="store_true")
    return result


def main() -> None:
    train(parser().parse_args())


if __name__ == "__main__":
    main()
