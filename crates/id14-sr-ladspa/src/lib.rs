//! Allocation-free LADSPA callback adapter for the fixed-size `id14-sr` engine.

use id14_sr::{StreamingSr, CHANNELS, CHUNK_FRAMES, SAMPLE_RATE};
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::os::raw::{c_char, c_int, c_ulong, c_void};
use std::ptr;
use std::sync::Arc;

const INPUT_L: usize = 0;
const INPUT_R: usize = 1;
const OUTPUT_L: usize = 2;
const OUTPUT_R: usize = 3;
const PORT_COUNT: usize = 4;
const DETECTOR_LOW_FIRST: usize = 4;
const DETECTOR_LOW_LAST: usize = 64;
const DETECTOR_HIGH_FIRST: usize = 72;
const DETECTOR_HIGH_LAST: usize = 106;
const BANDLIMITED_RATIO: f32 = 0.0002;
const FULLBAND_RATIO: f32 = 0.002;
const MIX_ATTACK_PER_CHUNK: f32 = 0.125;
const MIX_RELEASE_PER_CHUNK: f32 = 0.5;

/// Fixed latency added by the arbitrary-block adapter itself.
pub const ADAPTER_DELAY_FRAMES: usize = CHUNK_FRAMES;

struct BandwidthDetector {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
    spectrum: [Complex32; CHUNK_FRAMES],
    window: [f32; CHUNK_FRAMES],
    mix: f32,
}

impl BandwidthDetector {
    fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(CHUNK_FRAMES);
        let scratch = vec![Complex32::new(0.0, 0.0); fft.get_inplace_scratch_len()];
        let mut window = [0.0; CHUNK_FRAMES];
        for (index, value) in window.iter_mut().enumerate() {
            *value = 0.5 - 0.5 * (2.0 * PI * index as f32 / CHUNK_FRAMES as f32).cos();
        }
        Self {
            fft,
            scratch,
            spectrum: [Complex32::new(0.0, 0.0); CHUNK_FRAMES],
            window,
            mix: 0.0,
        }
    }

    fn channel_energy(&mut self, interleaved: &[f32], channel: usize) -> (f32, f32) {
        for frame in 0..CHUNK_FRAMES {
            self.spectrum[frame] = Complex32::new(
                interleaved[frame * CHANNELS + channel] * self.window[frame],
                0.0,
            );
        }
        self.fft
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);
        let low = self.spectrum[DETECTOR_LOW_FIRST..=DETECTOR_LOW_LAST]
            .iter()
            .map(Complex32::norm_sqr)
            .sum();
        let high = self.spectrum[DETECTOR_HIGH_FIRST..=DETECTOR_HIGH_LAST]
            .iter()
            .map(Complex32::norm_sqr)
            .sum();
        (low, high)
    }

    fn update(&mut self, interleaved: &[f32; CHUNK_FRAMES * CHANNELS]) -> f32 {
        let (low_l, high_l) = self.channel_energy(interleaved, 0);
        let (low_r, high_r) = self.channel_energy(interleaved, 1);
        let low = low_l + low_r;
        let high = high_l + high_r;
        let target = if low < 1e-8 {
            0.0
        } else {
            let ratio = high / low;
            if ratio <= BANDLIMITED_RATIO {
                1.0
            } else if ratio >= FULLBAND_RATIO {
                0.0
            } else {
                let position = (ratio.ln() - BANDLIMITED_RATIO.ln())
                    / (FULLBAND_RATIO.ln() - BANDLIMITED_RATIO.ln());
                1.0 - position.clamp(0.0, 1.0)
            }
        };
        if target < self.mix {
            self.mix = (self.mix - MIX_RELEASE_PER_CHUNK).max(target);
        } else {
            self.mix = (self.mix + MIX_ATTACK_PER_CHUNK).min(target);
        }
        self.mix
    }
}

/// Owns all callback state. Heap allocations happen only in `new`, which is
/// called from LADSPA `instantiate`, never from `run`.
pub struct Adapter {
    supported_rate: bool,
    engine: StreamingSr,
    detector: BandwidthDetector,
    input: [f32; CHUNK_FRAMES * CHANNELS],
    output: [f32; CHUNK_FRAMES * CHANNELS],
    input_frames: usize,
    output_frame: usize,
}

impl Adapter {
    pub fn new(sample_rate: usize) -> Self {
        Self {
            supported_rate: sample_rate == SAMPLE_RATE,
            engine: StreamingSr::new(),
            detector: BandwidthDetector::new(),
            input: [0.0; CHUNK_FRAMES * CHANNELS],
            output: [0.0; CHUNK_FRAMES * CHANNELS],
            input_frames: 0,
            output_frame: CHUNK_FRAMES,
        }
    }

    /// Adapts planar buffers and any host block length to 256-frame interleaved chunks.
    pub fn process(&mut self, left: &[f32], right: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let frames = left
            .len()
            .min(right.len())
            .min(out_l.len())
            .min(out_r.len());
        if !self.supported_rate {
            out_l[..frames].copy_from_slice(&left[..frames]);
            out_r[..frames].copy_from_slice(&right[..frames]);
            return;
        }
        for frame in 0..frames {
            if self.output_frame < CHUNK_FRAMES {
                out_l[frame] = self.output[self.output_frame * CHANNELS];
                out_r[frame] = self.output[self.output_frame * CHANNELS + 1];
                self.output_frame += 1;
            } else {
                out_l[frame] = 0.0;
                out_r[frame] = 0.0;
            }

            self.input[self.input_frames * CHANNELS] = left[frame];
            self.input[self.input_frames * CHANNELS + 1] = right[frame];
            self.input_frames += 1;
            if self.input_frames == CHUNK_FRAMES {
                let mix = self.detector.update(&self.input);
                if self
                    .engine
                    .process_with_mix(&self.input, &mut self.output, mix)
                    .is_err()
                {
                    self.output.fill(0.0);
                }
                self.input_frames = 0;
                self.output_frame = 0;
            }
        }
    }

    #[cfg(test)]
    fn completion_mix(&self) -> f32 {
        self.detector.mix
    }
}

#[repr(C)]
struct LadspaPortRangeHint {
    hint_descriptor: c_int,
    lower_bound: f32,
    upper_bound: f32,
}

#[repr(C)]
pub struct LadspaDescriptor {
    unique_id: c_ulong,
    label: *const c_char,
    properties: c_int,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: c_ulong,
    port_descriptors: *const c_int,
    port_names: *const *const c_char,
    port_range_hints: *const LadspaPortRangeHint,
    implementation_data: *mut c_void,
    instantiate: Option<unsafe extern "C" fn(*const LadspaDescriptor, c_ulong) -> *mut c_void>,
    connect_port: Option<unsafe extern "C" fn(*mut c_void, c_ulong, *mut f32)>,
    activate: Option<unsafe extern "C" fn(*mut c_void)>,
    run: Option<unsafe extern "C" fn(*mut c_void, c_ulong)>,
    run_adding: Option<unsafe extern "C" fn(*mut c_void, c_ulong)>,
    set_run_adding_gain: Option<unsafe extern "C" fn(*mut c_void, f32)>,
    deactivate: Option<unsafe extern "C" fn(*mut c_void)>,
    cleanup: Option<unsafe extern "C" fn(*mut c_void)>,
}

unsafe impl Sync for LadspaDescriptor {}

struct Instance {
    adapter: Adapter,
    ports: [*mut f32; PORT_COUNT],
}

unsafe extern "C" fn instantiate(
    _descriptor: *const LadspaDescriptor,
    sample_rate: c_ulong,
) -> *mut c_void {
    match std::panic::catch_unwind(|| {
        Box::new(Instance {
            adapter: Adapter::new(sample_rate as usize),
            ports: [ptr::null_mut(); PORT_COUNT],
        })
    }) {
        Ok(instance) => Box::into_raw(instance).cast(),
        Err(_) => ptr::null_mut(),
    }
}

unsafe extern "C" fn connect_port(instance: *mut c_void, port: c_ulong, data: *mut f32) {
    if instance.is_null() || port as usize >= PORT_COUNT {
        return;
    }
    // SAFETY: LADSPA owns the live instance and calls this with the handle returned above.
    unsafe { &mut *instance.cast::<Instance>() }.ports[port as usize] = data;
}

unsafe extern "C" fn run(instance: *mut c_void, sample_count: c_ulong) {
    if instance.is_null() || sample_count == 0 {
        return;
    }
    // SAFETY: the LADSPA host connects all four ports before run and guarantees
    // each buffer contains SampleCount f32 values for the duration of this call.
    let instance = unsafe { &mut *instance.cast::<Instance>() };
    if instance.ports.iter().any(|port| port.is_null()) {
        return;
    }
    let frames = sample_count as usize;
    let input_l = unsafe { std::slice::from_raw_parts(instance.ports[INPUT_L], frames) };
    let input_r = unsafe { std::slice::from_raw_parts(instance.ports[INPUT_R], frames) };
    let output_l = unsafe { std::slice::from_raw_parts_mut(instance.ports[OUTPUT_L], frames) };
    let output_r = unsafe { std::slice::from_raw_parts_mut(instance.ports[OUTPUT_R], frames) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        instance
            .adapter
            .process(input_l, input_r, output_l, output_r);
    }));
    if result.is_err() {
        output_l.copy_from_slice(input_l);
        output_r.copy_from_slice(input_r);
    }
}

unsafe extern "C" fn cleanup(instance: *mut c_void) {
    if !instance.is_null() {
        // SAFETY: LADSPA calls cleanup once for the handle returned by instantiate.
        drop(unsafe { Box::from_raw(instance.cast::<Instance>()) });
    }
}

const AUDIO_INPUT: c_int = 0x1 | 0x8;
const AUDIO_OUTPUT: c_int = 0x2 | 0x8;
static PORT_DESCRIPTORS: [c_int; PORT_COUNT] =
    [AUDIO_INPUT, AUDIO_INPUT, AUDIO_OUTPUT, AUDIO_OUTPUT];
// Raw pointers do not implement Sync, so wrap only the immutable static name array.
#[repr(transparent)]
struct PortNames([*const c_char; PORT_COUNT]);
unsafe impl Sync for PortNames {}
static PORT_NAMES: PortNames = PortNames([
    b"Input L\0".as_ptr().cast(),
    b"Input R\0".as_ptr().cast(),
    b"Output L\0".as_ptr().cast(),
    b"Output R\0".as_ptr().cast(),
]);

static PORT_HINTS: [LadspaPortRangeHint; PORT_COUNT] = [
    LadspaPortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
    LadspaPortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
    LadspaPortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
    LadspaPortRangeHint {
        hint_descriptor: 0,
        lower_bound: 0.0,
        upper_bound: 0.0,
    },
];

static DESCRIPTOR: LadspaDescriptor = LadspaDescriptor {
    unique_id: 0x4944_31,
    label: b"id14_sr_stereo\0".as_ptr().cast(),
    // Our four Rust slices must not alias. LADSPA hosts honor this flag by
    // providing distinct input and output buffers.
    properties: 0x2,
    name: b"iD14 Stereo High-Frequency Completion\0".as_ptr().cast(),
    maker: b"audient-id14-linux contributors\0".as_ptr().cast(),
    copyright: b"MIT\0".as_ptr().cast(),
    port_count: PORT_COUNT as c_ulong,
    port_descriptors: PORT_DESCRIPTORS.as_ptr(),
    port_names: PORT_NAMES.0.as_ptr(),
    port_range_hints: PORT_HINTS.as_ptr(),
    implementation_data: ptr::null_mut(),
    instantiate: Some(instantiate),
    connect_port: Some(connect_port),
    activate: None,
    run: Some(run),
    run_adding: None,
    set_run_adding_gain: None,
    deactivate: None,
    cleanup: Some(cleanup),
};

/// LADSPA 1.1 plugin entry point.
#[no_mangle]
pub extern "C" fn ladspa_descriptor(index: c_ulong) -> *const LadspaDescriptor {
    if index == 0 {
        &DESCRIPTOR
    } else {
        ptr::null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_tones(adapter: &mut Adapter, chunks: usize, low_hz: f32, high_hz: Option<f32>) {
        for chunk in 0..chunks {
            let mut left = [0.0; CHUNK_FRAMES];
            let mut right = [0.0; CHUNK_FRAMES];
            let mut out_l = [0.0; CHUNK_FRAMES];
            let mut out_r = [0.0; CHUNK_FRAMES];
            for frame in 0..CHUNK_FRAMES {
                let index = chunk * CHUNK_FRAMES + frame;
                let mut sample =
                    (2.0 * PI * low_hz * index as f32 / SAMPLE_RATE as f32).sin() * 0.2;
                if let Some(high_hz) = high_hz {
                    sample += (2.0 * PI * high_hz * index as f32 / SAMPLE_RATE as f32).sin() * 0.1;
                }
                left[frame] = sample;
                right[frame] = sample;
            }
            adapter.process(&left, &right, &mut out_l, &mut out_r);
        }
    }

    #[test]
    fn detector_enables_completion_for_12k_bandlimited_material() {
        let mut adapter = Adapter::new(SAMPLE_RATE);
        feed_tones(&mut adapter, 12, 3_000.0, None);
        assert!(adapter.completion_mix() > 0.99);
    }

    #[test]
    fn detector_preserves_fullband_material_and_releases_quickly() {
        let mut adapter = Adapter::new(SAMPLE_RATE);
        feed_tones(&mut adapter, 12, 3_000.0, None);
        assert!(adapter.completion_mix() > 0.99);
        feed_tones(&mut adapter, 2, 3_000.0, Some(16_000.0));
        assert!(adapter.completion_mix() < 0.01);
    }

    #[test]
    fn arbitrary_host_blocks_match_fixed_host_blocks() {
        let sizes = [1, 7, 128, 511, 13, 1024, 3];
        let schedule: Vec<usize> = sizes.into_iter().cycle().take(40).collect();
        let total: usize = schedule.iter().sum();
        let left: Vec<f32> = (0..total)
            .map(|index| (index as f32 * 0.017).sin() * 0.2)
            .collect();
        let right: Vec<f32> = (0..total)
            .map(|index| (index as f32 * 0.031).cos() * 0.15)
            .collect();

        let mut arbitrary = Adapter::new(SAMPLE_RATE);
        let mut arbitrary_output = Vec::with_capacity(total);
        let mut offset = 0;
        for size in schedule {
            let mut out_l = vec![0.0; size];
            let mut out_r = vec![0.0; size];
            arbitrary.process(
                &left[offset..offset + size],
                &right[offset..offset + size],
                &mut out_l,
                &mut out_r,
            );
            arbitrary_output.extend(out_l.into_iter().zip(out_r));
            offset += size;
        }

        let mut fixed = Adapter::new(SAMPLE_RATE);
        let mut fixed_output = Vec::with_capacity(total);
        for offset in (0..total).step_by(CHUNK_FRAMES) {
            let end = (offset + CHUNK_FRAMES).min(total);
            let mut out_l = vec![0.0; end - offset];
            let mut out_r = vec![0.0; end - offset];
            fixed.process(
                &left[offset..end],
                &right[offset..end],
                &mut out_l,
                &mut out_r,
            );
            fixed_output.extend(out_l.into_iter().zip(out_r));
        }
        assert_eq!(arbitrary_output, fixed_output);
    }

    #[test]
    fn unsupported_sample_rate_is_safe_passthrough() {
        let mut adapter = Adapter::new(44_100);
        let left = [0.1, 0.2, 0.3];
        let right = [-0.1, -0.2, -0.3];
        let mut out_l = [0.0; 3];
        let mut out_r = [0.0; 3];
        adapter.process(&left, &right, &mut out_l, &mut out_r);
        assert_eq!(left, out_l);
        assert_eq!(right, out_r);
    }

    #[test]
    fn adapter_and_engine_delay_is_exactly_1024_frames() {
        let mut adapter = Adapter::new(SAMPLE_RATE);
        let mut observed = Vec::new();
        for chunk in 0..8 {
            let mut left = [0.0; CHUNK_FRAMES];
            let mut right = [0.0; CHUNK_FRAMES];
            let mut out_l = [0.0; CHUNK_FRAMES];
            let mut out_r = [0.0; CHUNK_FRAMES];
            if chunk == 0 {
                left[0] = 1.0;
                right[0] = 1.0;
            }
            adapter.process(&left, &right, &mut out_l, &mut out_r);
            observed.extend(out_l);
        }
        let peak = observed
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .map(|(index, _)| index)
            .unwrap();
        assert_eq!(peak, 1024);
        assert!((observed[peak] - 1.0).abs() < 0.0001);
    }

    #[test]
    fn descriptor_has_one_stereo_plugin() {
        assert!(!ladspa_descriptor(0).is_null());
        assert!(ladspa_descriptor(1).is_null());
    }
}
