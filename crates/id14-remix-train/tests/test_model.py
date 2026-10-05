import copy

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


def test_sequence_matches_repeated_streaming_steps():
    torch.manual_seed(1407)
    model = RemixNet("131k").eval()
    x = torch.randn(2, 7, 4, 513)
    initial = torch.randn(2, model.state_size).tanh()
    with torch.no_grad():
        sequence_masks, sequence_state = model.forward_sequence(x, initial)
        state = initial
        step_masks = []
        for frame in range(x.shape[1]):
            mask, state = model(x[:, frame], state)
            step_masks.append(mask)
        step_masks = torch.stack(step_masks, dim=1)
    mask_difference = float((sequence_masks - step_masks).abs().max())
    state_difference = float((sequence_state - state).abs().max())
    print(
        f"sequence_step_max_mask_difference={mask_difference:.9g} "
        f"sequence_step_max_state_difference={state_difference:.9g}"
    )
    assert mask_difference < 2e-6
    assert state_difference < 2e-6


def test_sequence_gradients_match_repeated_streaming_steps():
    torch.manual_seed(1408)
    sequence_model = RemixNet("131k")
    step_model = copy.deepcopy(sequence_model)
    x = torch.randn(1, 4, 4, 513)
    initial = torch.zeros(1, sequence_model.state_size)
    sequence_masks, sequence_state = sequence_model.forward_sequence(x, initial)
    (sequence_masks.square().mean() + sequence_state.square().mean()).backward()
    state = initial
    step_masks = []
    for frame in range(x.shape[1]):
        mask, state = step_model(x[:, frame], state)
        step_masks.append(mask)
    stacked = torch.stack(step_masks, dim=1)
    (stacked.square().mean() + state.square().mean()).backward()
    differences = []
    for sequence_parameter, step_parameter in zip(
        sequence_model.parameters(), step_model.parameters(), strict=True
    ):
        differences.append(float((sequence_parameter.grad - step_parameter.grad).abs().max()))
    maximum = max(differences)
    print(f"sequence_step_max_gradient_difference={maximum:.9g}")
    assert maximum < 2e-6


def test_pre_sequence_grucell_checkpoint_loads_without_weight_changes():
    model = RemixNet("131k")
    current = model.state_dict()
    new_to_old = {
        "gru.weight_ih_l0": "gru.weight_ih",
        "gru.weight_hh_l0": "gru.weight_hh",
        "gru.bias_ih_l0": "gru.bias_ih",
        "gru.bias_hh_l0": "gru.bias_hh",
    }
    old_checkpoint = {
        new_to_old.get(name, name): value.clone() for name, value in current.items()
    }
    loaded = RemixNet("131k")
    result = loaded.load_state_dict(old_checkpoint)
    assert not result.missing_keys
    assert not result.unexpected_keys
    for name, expected in current.items():
        torch.testing.assert_close(loaded.state_dict()[name], expected)


def test_sequence_gru_matches_original_grucell_math():
    torch.manual_seed(1409)
    model = RemixNet("131k").eval()
    cell = torch.nn.GRUCell(model.gru.input_size, model.gru.hidden_size)
    with torch.no_grad():
        cell.weight_ih.copy_(model.gru.weight_ih_l0)
        cell.weight_hh.copy_(model.gru.weight_hh_l0)
        cell.bias_ih.copy_(model.gru.bias_ih_l0)
        cell.bias_hh.copy_(model.gru.bias_hh_l0)
        values = torch.randn(2, 9, model.gru.input_size)
        initial = torch.randn(2, model.state_size).tanh()
        sequence, final = model.gru(values, initial.unsqueeze(0))
        state = initial
        original = []
        for frame in range(values.shape[1]):
            state = cell(values[:, frame], state)
            original.append(state)
        original = torch.stack(original, dim=1)
    value_difference = float((sequence - original).abs().max())
    state_difference = float((final.squeeze(0) - state).abs().max())
    print(
        f"sequence_gru_grucell_max_value_difference={value_difference:.9g} "
        f"sequence_gru_grucell_max_state_difference={state_difference:.9g}"
    )
    assert value_difference < 2e-6
    assert state_difference < 2e-6


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
