//! Reproducible black-box timing and sample comparison through Adapter's public API.
use id14_sr_ladspa::Adapter;
use std::hint::black_box;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::Instant;

const RATE: usize = 48_000;
const FRAMES: usize = 256;
const BLOCKS: usize = RATE * 40 / FRAMES;
const TAIL_BLOCKS: usize = RATE * 10 / FRAMES;
const CASES: [&str; 3] = ["sound", "zero", "sound_then_zero"];

fn input(case: &str, block: usize, left: &mut [f32], right: &mut [f32]) {
    for i in 0..FRAMES {
        let frame = block * FRAMES + i;
        if case == "zero" || (case == "sound_then_zero" && frame >= RATE) {
            left[i] = 0.0;
            right[i] = 0.0;
        } else {
            let t = frame as f64 / RATE as f64;
            left[i] = (0.25 * (std::f64::consts::TAU * 73.0 * t).sin()
                + 0.10 * (std::f64::consts::TAU * 997.0 * t).sin()
                + 0.04 * (std::f64::consts::TAU * 9011.0 * t).sin()) as f32;
            right[i] = (0.22 * (std::f64::consts::TAU * 91.0 * t + 0.3).sin()
                - 0.09 * (std::f64::consts::TAU * 1999.0 * t).sin()
                + 0.03 * (std::f64::consts::TAU * 11003.0 * t).sin()) as f32;
        }
    }
}

fn ordered_bits(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits >> 31 != 0 { !bits } else { bits | (1 << 31) }
}

fn samples(mode: &str, directory: &Path) -> std::io::Result<()> {
    for mix in [0, 100, 150, 200] {
        for case in CASES {
            let path = directory.join(format!("{case}-{mix}.f32le"));
            let mut writer = if mode == "record" {
                Some(BufWriter::new(std::fs::File::create(&path)?))
            } else { None };
            let mut reader = if mode == "compare" {
                Some(BufReader::new(std::fs::File::open(&path)?))
            } else { None };
            let mut adapter = Adapter::new(RATE);
            let (mut l, mut r, mut ol, mut or) = ([0.0; FRAMES], [0.0; FRAMES], [0.0; FRAMES], [0.0; FRAMES]);
            let (mut changed, mut max_ulp, mut max_abs) = (0u64, 0u32, 0.0f64);
            for block in 0..BLOCKS {
                input(case, block, &mut l, &mut r);
                adapter.process_with_user_mix(&l, &r, &mut ol, &mut or, mix as f32);
                for value in ol.iter().chain(or.iter()) {
                    assert!(value.is_finite(), "nonfinite output");
                    if let Some(w) = writer.as_mut() { w.write_all(&value.to_le_bytes())?; }
                    if let Some(r) = reader.as_mut() {
                        let mut bytes = [0; 4];
                        r.read_exact(&mut bytes)?;
                        let before = f32::from_le_bytes(bytes);
                        assert!(before.is_finite(), "nonfinite baseline");
                        changed += u64::from(before.to_bits() != value.to_bits());
                        max_ulp = max_ulp.max(ordered_bits(before).abs_diff(ordered_bits(*value)));
                        max_abs = max_abs.max((before as f64 - *value as f64).abs());
                    }
                }
            }
            if let Some(mut w) = writer { w.flush()?; }
            if let Some(mut r) = reader {
                assert_eq!(r.read(&mut [0; 1])?, 0, "extra baseline samples");
                println!("DIFF mix={mix} case={case} samples={} changed_bits_samples={changed} max_ulp={max_ulp} max_abs={max_abs:.12e}", BLOCKS * FRAMES * 2);
            }
        }
    }
    Ok(())
}

fn timing() {
    for mix in [0, 150] {
        for case in CASES {
            let mut trials = Vec::new();
            for _ in 0..5 {
                let mut adapter = Adapter::new(RATE);
                let (mut l, mut r, mut ol, mut or) = ([0.0; FRAMES], [0.0; FRAMES], [0.0; FRAMES], [0.0; FRAMES]);
                let mut tail_ns = 0u128;
                for block in 0..BLOCKS {
                    input(case, block, &mut l, &mut r);
                    let start = Instant::now();
                    adapter.process_with_user_mix(black_box(&l), black_box(&r), &mut ol, &mut or, mix as f32);
                    let elapsed = start.elapsed().as_nanos();
                    black_box((&ol, &or));
                    if block >= BLOCKS - TAIL_BLOCKS { tail_ns += elapsed; }
                }
                trials.push(tail_ns as f64 / TAIL_BLOCKS as f64);
            }
            trials.sort_by(f64::total_cmp);
            println!("TIME mix={mix} case={case} ns_per_256_median={:.1} min={:.1} max={:.1}", trials[2], trials[0], trials[4]);
        }
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() == 3 && matches!(args[1].as_str(), "record" | "compare"), "usage: silent_load_probe record|compare EVIDENCE_DIRECTORY");
    let directory = Path::new(&args[2]);
    std::fs::create_dir_all(directory)?;
    println!("CONFIG rate={RATE} block_frames={FRAMES} seconds=40 tail_seconds=10 timing_trials=5 mode={}", args[1]);
    timing();
    samples(&args[1], directory)
}
