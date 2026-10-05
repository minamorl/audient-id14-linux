//! Slow, stereo-linked, feed-forward loudness matching of the correction band.
//! The dry path is never filtered or multiplied. BS.1770's K response is evaluated
//! at STFT bins; the independent verification meter uses time-domain IIRs/gating.
use crate::dsp::{BassGuard, Complex, BINS, FFT, HOP, RATE};

const AVERAGE_SECONDS: f64 = 3.0;
const FOLLOW_SECONDS: f64 = 0.5;
const SLEW_DB_PER_SECOND: f64 = 0.5;
const MAX_DB: f64 = 3.0;

fn response_power(b: [f64; 3], a: [f64; 3], omega: f64) -> f64 {
    let norm = |c: [f64; 3]| {
        let re = c[0] + c[1] * omega.cos() + c[2] * (2.0 * omega).cos();
        let im = c[1] * omega.sin() + c[2] * (2.0 * omega).sin();
        re * re + im * im
    };
    norm(b) / norm(a)
}

pub struct LoudnessMatch {
    weights: [f64; BINS],
    guard: [f64; BINS],
    // E[(base + gain * remixed_high)^2 - dry^2] = A*g^2 + 2*B*g + C.
    moments: [f64; 3],
    amounts: [f32; 4],
    db: f64,
    average: f64,
    follow: f64,
}
impl LoudnessMatch {
    pub fn new(guard: &BassGuard) -> Self {
        Self {
            weights: std::array::from_fn(|k| {
                let omega = 2.0 * std::f64::consts::PI * k as f64 / FFT as f64;
                response_power(
                    [1.53512485958697, -2.69169618940638, 1.19839281085285],
                    [1.0, -1.69065929318241, 0.73248077421585],
                    omega,
                ) * response_power(
                    [1.0, -2.0, 1.0],
                    [1.0, -1.99004745483398, 0.99007225036621],
                    omega,
                ) * if k == FFT / 2 { 1.0 } else { 2.0 }
            }),
            guard: std::array::from_fn(|k| guard.response_at_bin(k)),
            moments: [0.0; 3],
            amounts: [0.0; 4],
            db: 0.0,
            average: 1.0 - (-(HOP as f64) / RATE as f64 / AVERAGE_SECONDS).exp(),
            follow: 1.0 - (-(HOP as f64) / RATE as f64 / FOLLOW_SECONDS).exp(),
        }
    }
    pub fn db(&self) -> f32 {
        self.db as f32
    }

    pub fn gain(
        &mut self,
        spectrum: &[[Complex; FFT]; 2],
        mask: &[f32; 4 * BINS],
        amounts: [f32; 4],
        valid: bool,
    ) -> f64 {
        if amounts == [0.0; 4] {
            self.moments = [0.0; 3];
            self.amounts = amounts;
            self.db = 0.0;
            return 1.0;
        }
        if amounts != self.amounts {
            // Old amount statistics cannot predict a new mixture. Retain the
            // envelope itself, so a nonzero control change cannot jump the gain.
            self.moments = [0.0; 3];
            self.amounts = amounts;
        }
        if valid {
            let gains = amounts.map(|x| 10_f64.powf(x as f64 / 20.0) - 1.0);
            let mut moments = [0.0; 3];
            let mut energy = 0.0;
            for k in 7..BINS {
                let power = spectrum
                    .iter()
                    .map(|s| s[k].re * s[k].re + s[k].im * s[k].im)
                    .sum::<f64>()
                    * self.weights[k];
                let remix = 1.0
                    + (0..4)
                        .map(|p| gains[p] * mask[p * BINS + k] as f64)
                        .sum::<f64>();
                let base = 1.0 - self.guard[k];
                let high = self.guard[k] * remix;
                moments[0] += power * high * high;
                moments[1] += power * base * high;
                moments[2] += power * (base * base - 1.0);
                energy += power;
            }
            // Freeze through silence/very quiet input. FFT sqrt-Hann energy
            // normalization is N^2/2; 1e-7 is approximately the -70 LKFS gate.
            if energy * 2.0 / (FFT * FFT) as f64 > 1e-7 && moments.iter().all(|x| x.is_finite()) {
                for (average, value) in self.moments.iter_mut().zip(moments) {
                    *average += self.average * (value - *average);
                }
                let [a, b, c] = self.moments;
                if a > 1e-20 {
                    let gain = (-b + (b * b - a * c).max(0.0).sqrt()) / a;
                    if gain > 0.0 && gain.is_finite() {
                        let target = (20.0 * gain.log10()).clamp(-MAX_DB, MAX_DB);
                        let step = SLEW_DB_PER_SECOND * HOP as f64 / RATE as f64;
                        self.db += (self.follow * (target - self.db)).clamp(-step, step);
                    }
                }
            }
        }
        10_f64.powf(self.db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_gain_directions_converge_without_fast_modulation() {
        for part in [0, 3] {
            let mut controller = LoudnessMatch::new(&BassGuard::default());
            let mut spectrum = [[Complex::default(); FFT]; 2];
            spectrum[0][64].re = 100.0;
            spectrum[1][64].im = 70.0;
            let mut mask = [0.0; 4 * BINS];
            mask[part * BINS..(part + 1) * BINS].fill(1.0);
            let mut previous = 0.0_f32;
            let mut max_step = 0.0_f32;
            for _ in 0..(20 * RATE / HOP) {
                controller.gain(&spectrum, &mask, [3.0, 0.0, 0.0, -3.0], true);
                max_step = max_step.max((controller.db() - previous).abs());
                previous = controller.db();
            }
            let expected = if part == 0 { -3.0 } else { 3.0 };
            println!(
                "loudness part={part} settled_db={} max_hop_step_db={max_step}",
                controller.db()
            );
            assert!((controller.db() - expected).abs() < 1e-4);
            assert!(max_step <= (SLEW_DB_PER_SECOND * HOP as f64 / RATE as f64) as f32 + 3e-7);
            let before = controller.db();
            controller.gain(
                &[[Complex::default(); FFT]; 2],
                &mask,
                [3.0, 0.0, 0.0, -3.0],
                true,
            );
            assert_eq!(controller.db(), before);
            assert_eq!(controller.gain(&spectrum, &mask, [0.0; 4], true), 1.0);
        }
    }
}
