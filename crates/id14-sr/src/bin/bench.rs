use id14_sr::{Bandwidth, StreamingSr, CHANNELS, CHUNK_FRAMES, SAMPLE_RATE};
use std::time::Instant;

fn main() {
    let mut engine = StreamingSr::new();
    let mut input = [0.0_f32; CHUNK_FRAMES * CHANNELS];
    let mut output = [0.0_f32; CHUNK_FRAMES * CHANNELS];
    for frame in 0..CHUNK_FRAMES {
        input[frame * 2] = (frame as f32 * 0.13).sin() * 0.2;
        input[frame * 2 + 1] = (frame as f32 * 0.17).cos() * 0.2;
    }
    for _ in 0..100 {
        engine
            .process(&input, &mut output, true, Bandwidth::BandLimited)
            .unwrap();
    }
    let mut micros = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let start = Instant::now();
        engine
            .process(&input, &mut output, true, Bandwidth::BandLimited)
            .unwrap();
        micros.push(start.elapsed().as_secs_f64() * 1e6);
    }
    micros.sort_by(f64::total_cmp);
    println!(
        "sample_rate={} frames={} budget_us={:.2} median_us={:.2} p95_us={:.2} max_us={:.2}",
        SAMPLE_RATE,
        CHUNK_FRAMES,
        CHUNK_FRAMES as f64 / SAMPLE_RATE as f64 * 1e6,
        micros[500],
        micros[950],
        micros[999]
    );
}
