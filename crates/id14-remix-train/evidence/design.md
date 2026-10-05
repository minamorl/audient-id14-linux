# Design decisions against the research starting point

The research direction proposed a causal TFC-TDF U-Net, left-padded temporal
convolutions, and a bounded GRU. The committed checkpoint architecture contains
frequency convolutions and a bottleneck TDF but no temporal-convolution weights;
all temporal memory belongs to its GRU state. The sequence optimization therefore
folds independent frequency convolutions across batch and time, then makes one
causal `nn.GRU` call over time. It does not invent temporal convolution weights
or enlarge the streaming state, so old checkpoints and the `remix-v1` ABI retain
their learned function. The GRU is causal and its hidden state is bounded when
initialized to zero.

Lookahead is learned by alignment rather than a non-causal operator: call `t`
receives frame `t`, advances the recurrent state, and is scored against frame
`t-Q`. Inference discards the first Q emissions and aligns the remainder to
frames starting at zero. Evaluation carries state through every track and
computes per-frame SDR, so a block reset cannot conceal recurrent degradation.

The 131K and 444K labels are realized as 130,993 and 443,974 trainable
parameters. Both use the same graph and ABI; this isolates capacity from
lookahead in the six-run matrix.

Primary implementation references read for this optimization:

- <https://docs.pytorch.org/docs/2.14/generated/torch.nn.GRU.html>
- <https://docs.pytorch.org/docs/2.14/generated/torch.nn.GRUCell.html>
- <https://docs.pytorch.org/docs/2.14/onnx.html>
