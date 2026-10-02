//! Offline renderer for aligned A/B evidence. Input must be stereo 48 kHz f32 WAV.
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use id14_sr::{
    Bandwidth, StreamingSr, ALGORITHM_DELAY_FRAMES, CHANNELS, CHUNK_FRAMES, SAMPLE_RATE,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let input_path = arguments
        .next()
        .ok_or("usage: sr-render INPUT.wav OUTPUT.wav")?;
    let output_path = arguments
        .next()
        .ok_or("usage: sr-render INPUT.wav OUTPUT.wav")?;
    if arguments.next().is_some() {
        return Err("usage: sr-render INPUT.wav OUTPUT.wav".into());
    }
    let mut reader = WavReader::open(&input_path)?;
    let spec = reader.spec();
    if spec.channels as usize != CHANNELS
        || spec.sample_rate as usize != SAMPLE_RATE
        || spec.sample_format != SampleFormat::Float
        || spec.bits_per_sample != 32
    {
        return Err("input must be 48 kHz stereo f32 WAV".into());
    }
    let mut input: Vec<f32> = reader.samples::<f32>().collect::<Result<_, _>>()?;
    let original_samples = input.len();
    input.resize(
        input.len().div_ceil(CHUNK_FRAMES * CHANNELS) * CHUNK_FRAMES * CHANNELS,
        0.0,
    );
    let mut engine = StreamingSr::new();
    let mut rendered = Vec::with_capacity(input.len() + ALGORITHM_DELAY_FRAMES * CHANNELS);
    for chunk in input.chunks_exact(CHUNK_FRAMES * CHANNELS) {
        let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
        engine
            .process(chunk, &mut output, true, Bandwidth::BandLimited)
            .map_err(|_| "callback error")?;
        rendered.extend_from_slice(&output);
    }
    for _ in 0..ALGORITHM_DELAY_FRAMES / CHUNK_FRAMES {
        let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
        engine
            .process(
                &[0.0; CHUNK_FRAMES * CHANNELS],
                &mut output,
                true,
                Bandwidth::BandLimited,
            )
            .map_err(|_| "callback error")?;
        rendered.extend_from_slice(&output);
    }
    let aligned = &rendered
        [ALGORITHM_DELAY_FRAMES * CHANNELS..ALGORITHM_DELAY_FRAMES * CHANNELS + original_samples];
    let mut writer = WavWriter::create(
        output_path,
        WavSpec {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE as u32,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        },
    )?;
    for sample in aligned {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}
