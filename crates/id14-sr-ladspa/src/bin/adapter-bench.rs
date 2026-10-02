use id14_sr::{CHUNK_FRAMES, SAMPLE_RATE};
use id14_sr_ladspa::{Adapter, ADAPTER_DELAY_FRAMES};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let mut adapter = Adapter::new(SAMPLE_RATE);
    let mut left = [0.0_f32; CHUNK_FRAMES];
    let mut right = [0.0_f32; CHUNK_FRAMES];
    let mut out_l = [0.0_f32; CHUNK_FRAMES];
    let mut out_r = [0.0_f32; CHUNK_FRAMES];
    for frame in 0..CHUNK_FRAMES {
        left[frame] = (frame as f32 * 0.13).sin() * 0.2;
        right[frame] = (frame as f32 * 0.17).cos() * 0.2;
    }
    for _ in 0..100 {
        adapter.process(&left, &right, &mut out_l, &mut out_r);
    }
    let mut micros = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let start = Instant::now();
        adapter.process(
            black_box(&left),
            black_box(&right),
            black_box(&mut out_l),
            black_box(&mut out_r),
        );
        micros.push(start.elapsed().as_secs_f64() * 1e6);
    }
    micros.sort_by(f64::total_cmp);
    println!(
        "sample_rate={} frames={} budget_us={:.2} adapter_delay_frames={} median_us={:.2} p95_us={:.2} max_us={:.2}",
        SAMPLE_RATE,
        CHUNK_FRAMES,
        CHUNK_FRAMES as f64 / SAMPLE_RATE as f64 * 1e6,
        ADAPTER_DELAY_FRAMES,
        micros[500],
        micros[950],
        micros[999]
    );
}
