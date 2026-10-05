import torch

from remix_train.train import aligned_training_tensors, pack_real_sequence, remix_loss


def test_filter_loss_is_zero_for_exact_partition():
    mixture = torch.randn(2, 2, 513, 2)
    masks = torch.softmax(torch.randn(2, 4, 513), dim=1)
    sources = mixture.unsqueeze(1) * masks.unsqueeze(2).unsqueeze(-1)
    gains = torch.tensor([[1.2, 0.7, 1.0, 1.4], [0.8, 1.1, 1.3, 0.6]])
    loss = remix_loss(masks, mixture, sources, gains)
    assert loss < 1e-10


def test_sequence_alignment_and_flattened_loss_match_frame_loop():
    torch.manual_seed(1410)
    batch, frames, lookahead = 2, 6, 2
    mixture = torch.randn(batch, 2, frames, 513, 2)
    sources = torch.randn(batch, 4, 2, frames, 513, 2)
    masks = torch.softmax(torch.randn(batch, frames, 4, 513), dim=2)
    aligned_masks, aligned_mixture, aligned_sources = aligned_training_tensors(
        masks, mixture, sources, lookahead
    )
    gains = torch.rand(batch, frames - lookahead, 4) + 0.5
    loop = []
    for frame in range(frames - lookahead):
        loop.append(
            remix_loss(
                masks[:, frame + lookahead],
                mixture[:, :, frame],
                sources[:, :, :, frame],
                gains[:, frame],
            )
        )
    examples = batch * (frames - lookahead)
    flattened = remix_loss(
        aligned_masks.reshape(examples, 4, 513),
        aligned_mixture.reshape(examples, 2, 513, 2),
        aligned_sources.reshape(examples, 4, 2, 513, 2),
        gains.reshape(examples, 4),
    )
    torch.testing.assert_close(flattened, torch.stack(loop).mean())


def test_pack_real_sequence_matches_streaming_abi_order():
    values = torch.arange(2 * 2 * 3 * 513 * 2, dtype=torch.float32).reshape(
        2, 2, 3, 513, 2
    )
    packed = pack_real_sequence(values)
    assert packed.shape == (2, 3, 4, 513)
    torch.testing.assert_close(packed[:, :, 0], values[:, 0, :, :, 0])
    torch.testing.assert_close(packed[:, :, 1], values[:, 0, :, :, 1])
    torch.testing.assert_close(packed[:, :, 2], values[:, 1, :, :, 0])
    torch.testing.assert_close(packed[:, :, 3], values[:, 1, :, :, 1])
