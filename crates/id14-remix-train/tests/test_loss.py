import torch

from remix_train.train import remix_loss


def test_filter_loss_is_zero_for_exact_partition():
    mixture = torch.randn(2, 2, 513, 2)
    masks = torch.softmax(torch.randn(2, 4, 513), dim=1)
    sources = mixture.unsqueeze(1) * masks.unsqueeze(2).unsqueeze(-1)
    gains = torch.tensor([[1.2, 0.7, 1.0, 1.4], [0.8, 1.1, 1.3, 0.6]])
    loss = remix_loss(masks, mixture, sources, gains)
    assert loss < 1e-10
