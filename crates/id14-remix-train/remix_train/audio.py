"""The exact remix-v1 STFT and filter-form primitives."""

from __future__ import annotations

import torch
from torch import Tensor

SAMPLE_RATE = 48_000
N_FFT = 1024
HOP = 512
BINS = N_FFT // 2 + 1
STEMS = ("vocals", "drums", "bass", "other")


def sqrt_periodic_hann(*, device=None, dtype=torch.float32) -> Tensor:
    return torch.hann_window(N_FFT, periodic=True, device=device, dtype=dtype).sqrt()


def stft(audio: Tensor) -> Tensor:
    """Return complex [channels, frames, bins], without implicit padding."""
    return torch.stft(
        audio,
        n_fft=N_FFT,
        hop_length=HOP,
        window=sqrt_periodic_hann(device=audio.device, dtype=audio.dtype),
        center=False,
        return_complex=True,
    ).transpose(1, 2)


def istft(spectrum: Tensor, length: int) -> Tensor:
    """Square-root Hann synthesis by explicit overlap-add.

    PyTorch's center=False NOLA check rejects a window whose first sample is
    zero even though the steady-state 50% overlap is valid. Explicit synthesis
    also makes the boundary normalization and latency visible.
    """
    window = sqrt_periodic_hann(device=spectrum.device, dtype=spectrum.real.dtype)
    frames = torch.fft.irfft(spectrum, n=N_FFT, dim=2) * window
    output = torch.zeros(spectrum.shape[0], length, device=spectrum.device, dtype=frames.dtype)
    weight = torch.zeros(length, device=spectrum.device, dtype=frames.dtype)
    for index in range(frames.shape[1]):
        start = index * HOP
        stop = min(start + N_FFT, length)
        output[:, start:stop] += frames[:, index, : stop - start]
        weight[start:stop] += window[: stop - start].square()
    return output / weight.clamp_min(1e-8)


def pack_x(mixture_frame: Tensor) -> Tensor:
    """Map complex [batch,2,bins] to ABI [batch,4,bins]."""
    return torch.stack(
        (
            mixture_frame[:, 0].real,
            mixture_frame[:, 0].imag,
            mixture_frame[:, 1].real,
            mixture_frame[:, 1].imag,
        ),
        dim=1,
    )


def filter_form(mask: Tensor, mixture: Tensor, gains: Tensor) -> Tensor:
    """Apply X + sum((g_i-1)m_i X), preserving stereo position."""
    multiplier = 1.0 + ((gains - 1.0).unsqueeze(-1) * mask).sum(dim=1)
    return mixture * multiplier.unsqueeze(1)


def bass_protected_multiplier(mask: Tensor, gains: Tensor) -> Tensor:
    multiplier = 1.0 + ((gains - 1.0).unsqueeze(-1) * mask).sum(dim=1)
    cutoff_bin = int(300 * N_FFT / SAMPLE_RATE)
    multiplier[..., : cutoff_bin + 1] = 1.0
    return multiplier
