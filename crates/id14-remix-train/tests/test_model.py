import torch

from remix_train.audio import bass_protected_multiplier, istft, sqrt_periodic_hann, stft
from remix_train.model import RemixNet


def test_stream_shapes_and_mask_simplex():
    model = RemixNet("131k").eval()
    x = torch.randn(1, 4, 513)
    state = torch.zeros(1, model.state_size)
    with torch.no_grad():
        mask, state_out = model(x, state)
    assert mask.shape == (1, 4, 513)
    assert state_out.shape == state.shape
    assert torch.all(mask >= 0)
    torch.testing.assert_close(mask.sum(dim=1), torch.ones(1, 513))
    assert torch.all(state_out.abs() <= 1.0)


def test_periodic_sqrt_hann():
    window = sqrt_periodic_hann()
    torch.testing.assert_close(window.square(), torch.hann_window(1024, periodic=True))


def test_bass_bins_are_neutral():
    mask = torch.softmax(torch.randn(2, 4, 513), dim=1)
    gains = torch.tensor([[2.0, 1.0, 1.0, 0.5], [2.0, 1.0, 1.0, 0.5]])
    multiplier = bass_protected_multiplier(mask, gains)
    assert torch.equal(multiplier[:, :7], torch.ones(2, 7))


def test_stft_roundtrip_steady_state():
    signal = torch.randn(2, 4096)
    reconstructed = istft(stft(signal), signal.shape[1])
    # The first sample is multiplied by the exact Hann zero and is outside the
    # streaming steady state; every following sample must reconstruct.
    torch.testing.assert_close(reconstructed[:, 1:-1], signal[:, 1:-1], atol=2e-4, rtol=2e-4)


def test_declared_model_sizes_are_distinct_and_bounded():
    small = RemixNet("131k").parameter_count()
    large = RemixNet("444k").parameter_count()
    assert small == 130_993
    assert large == 443_974
