"""A frame-callable causal frequency U-Net with bounded recurrent state."""

from __future__ import annotations

from dataclasses import dataclass

import torch
from torch import Tensor, nn
import torch.nn.functional as F

BINS = 513
STEMS = 4


@dataclass(frozen=True)
class ModelConfig:
    base: int
    hidden: int


CONFIGS = {
    "131k": ModelConfig(base=21, hidden=26),  # 130,993 trainable parameters
    "444k": ModelConfig(base=37, hidden=115),  # 443,974 trainable parameters
}


def model_config(name: str) -> ModelConfig:
    try:
        return CONFIGS[name]
    except KeyError as exc:
        raise ValueError(f"unknown size {name!r}; expected one of {sorted(CONFIGS)}") from exc


class ConvBlock(nn.Module):
    def __init__(self, channels: int):
        super().__init__()
        self.net = nn.Sequential(
            nn.Conv1d(channels, channels, 3, padding=1),
            nn.SiLU(),
            nn.Conv1d(channels, channels, 3, padding=1),
        )
        self.norm = nn.GroupNorm(1, channels)

    def forward(self, x: Tensor) -> Tensor:
        return F.silu(self.norm(x + self.net(x)))


class RemixNet(nn.Module):
    """Streaming separator.

    Time is causal because only the current STFT frame and the previous GRU
    state are consumed. Frequency convolutions are non-causal by design. The
    recurrent state is bounded by tanh inside GRUCell. Lookahead is an export
    and target-alignment property: a Q model emits the mask for frame t-Q.
    """

    def __init__(self, size: str = "131k"):
        super().__init__()
        cfg = model_config(size)
        c, h = cfg.base, cfg.hidden
        self.size_name = size
        self.state_size = h
        self.input = nn.Conv1d(2, c, 5, padding=2)
        self.enc0 = ConvBlock(c)
        self.down1 = nn.Conv1d(c, 2 * c, 3, stride=2, padding=1)
        self.enc1 = ConvBlock(2 * c)
        self.down2 = nn.Conv1d(2 * c, 4 * c, 3, stride=2, padding=1)
        self.bottleneck = ConvBlock(4 * c)
        # TDF transformation at the 129-bin bottleneck, shared over channels.
        self.tdf = nn.Linear(129, 129)
        self.gru = nn.GRU(4 * c, h, batch_first=True)
        self.state_projection = nn.Linear(h, 4 * c)
        self.dec1 = nn.Conv1d(6 * c, 2 * c, 3, padding=1)
        self.dec1_block = ConvBlock(2 * c)
        self.dec0 = nn.Conv1d(3 * c, c, 3, padding=1)
        self.dec0_block = ConvBlock(c)
        self.output = nn.Conv1d(c, STEMS, 1)

    @staticmethod
    def _features(x: Tensor) -> Tensor:
        if x.ndim != 3 or x.shape[1] != 4 or x.shape[2] != BINS:
            raise ValueError(f"x must have [batch,4,{BINS}], got {tuple(x.shape)}")
        left = torch.sqrt(x[:, 0].square() + x[:, 1].square() + 1e-12)
        right = torch.sqrt(x[:, 2].square() + x[:, 3].square() + 1e-12)
        # Per-frame normalization makes the -40..0 dBFS augmentation useful
        # without throwing absolute level information away completely.
        mags = torch.stack((left, right), dim=1)
        return torch.log1p(10.0 * mags)

    def _frame_path(self, x: Tensor) -> tuple[Tensor, Tensor, Tensor]:
        """Run the non-recurrent U-Net encoder on independent frames."""
        e0 = self.enc0(F.silu(self.input(self._features(x))))
        e1 = self.enc1(F.silu(self.down1(e0)))
        z = self.bottleneck(F.silu(self.down2(e1)))
        return e0, e1, z + F.silu(self.tdf(z))

    def _decode(self, e0: Tensor, e1: Tensor, z: Tensor, state: Tensor) -> Tensor:
        z = z + self.state_projection(state).unsqueeze(2)
        up1 = F.interpolate(z, size=e1.shape[2], mode="linear", align_corners=False)
        up1 = self.dec1_block(F.silu(self.dec1(torch.cat((up1, e1), dim=1))))
        up0 = F.interpolate(up1, size=e0.shape[2], mode="linear", align_corners=False)
        up0 = self.dec0_block(F.silu(self.dec0(torch.cat((up0, e0), dim=1))))
        return torch.softmax(self.output(up0), dim=1)

    def forward_sequence(self, x: Tensor, state: Tensor) -> tuple[Tensor, Tensor]:
        """Run a contiguous sequence as one batched U-Net and one GRU call.

        `x` is `[batch,time,4,513]`; masks are `[batch,time,4,513]`.
        The convolutions are frequency-only and independent across time, so
        folding time into the batch is exactly the streaming computation. The
        GRU is the model's sole causal temporal operation.
        """
        if x.ndim != 4 or x.shape[2] != 4 or x.shape[3] != BINS:
            raise ValueError(f"x must have [batch,time,4,{BINS}], got {tuple(x.shape)}")
        if state.ndim != 2 or state.shape[1] != self.state_size:
            raise ValueError(
                f"state must have [batch,{self.state_size}], got {tuple(state.shape)}"
            )
        batch, frames = x.shape[:2]
        flat = x.reshape(batch * frames, 4, BINS)
        e0, e1, z = self._frame_path(flat)
        recurrent = z.mean(dim=2).reshape(batch, frames, -1)
        states, state_out = self.gru(recurrent, state.unsqueeze(0))
        masks = self._decode(e0, e1, z, states.reshape(batch * frames, -1))
        return masks.reshape(batch, frames, STEMS, BINS), state_out.squeeze(0)

    def forward(self, x: Tensor, state: Tensor) -> tuple[Tensor, Tensor]:
        if x.ndim != 3 or x.shape[1] != 4 or x.shape[2] != BINS:
            raise ValueError(f"x must have [batch,4,{BINS}], got {tuple(x.shape)}")
        masks, state_out = self.forward_sequence(x.unsqueeze(1), state)
        return masks[:, 0], state_out

    def load_state_dict(self, state_dict, strict: bool = True, assign: bool = False):
        """Accept the GRUCell keys used by checkpoints created before batching."""
        converted = dict(state_dict)
        old_to_new = {
            "gru.weight_ih": "gru.weight_ih_l0",
            "gru.weight_hh": "gru.weight_hh_l0",
            "gru.bias_ih": "gru.bias_ih_l0",
            "gru.bias_hh": "gru.bias_hh_l0",
        }
        for old, new in old_to_new.items():
            if old in converted and new not in converted:
                converted[new] = converted.pop(old)
        return super().load_state_dict(converted, strict=strict, assign=assign)

    def parameter_count(self) -> int:
        return sum(parameter.numel() for parameter in self.parameters())
