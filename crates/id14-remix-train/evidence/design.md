# Design decisions against the research starting point

The research direction proposed a causal TFC-TDF U-Net, left-padded temporal
convolutions, and a bounded GRU. `remix-v1` calls the model with exactly one
STFT frame, so the exported graph has no explicit temporal tensor dimension.
This implementation keeps the frequency U-Net and bottleneck TDF, and assigns
all temporal memory to the GRU state. It therefore has no temporal convolution
to left-pad. Adding one would require exporting its whole frame buffer inside
`state`, increasing the runtime copy and state surface without evidence that it
improves this small CPU-bound model. The GRU is causal and its hidden state is
bounded when initialized to zero.

Lookahead is learned by alignment rather than a non-causal operator: call `t`
receives frame `t`, advances the recurrent state, and is scored against frame
`t-Q`. Inference discards the first Q emissions and aligns the remainder to
frames starting at zero. Evaluation carries state through every track and
computes per-frame SDR, so a block reset cannot conceal recurrent degradation.

The 131K and 444K labels are realized as 130,993 and 443,974 trainable
parameters. Both use the same graph and ABI; this isolates capacity from
lookahead in the six-run matrix.
