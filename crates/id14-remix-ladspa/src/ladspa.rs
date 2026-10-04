//! LADSPA 1.1 ABI, translated from the official header. No inference occurs in run.
use crate::{
    dsp::{LATENCY, RATE},
    engine::{Controls, Engine, Status},
    model,
    state::{Publisher, Telemetry},
};
use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::path::PathBuf;
use std::sync::Arc;

const PORTS: usize = 11;
type Handle = *mut c_void;
#[repr(C)]
pub struct PortHint {
    pub descriptor: c_int,
    pub lower: f32,
    pub upper: f32,
}
#[repr(C)]
pub struct Descriptor {
    pub unique_id: c_ulong,
    pub label: *const c_char,
    pub properties: c_int,
    pub name: *const c_char,
    pub maker: *const c_char,
    pub copyright: *const c_char,
    pub port_count: c_ulong,
    pub port_descriptors: *const c_int,
    pub port_names: *const *const c_char,
    pub port_range_hints: *const PortHint,
    pub implementation_data: *mut c_void,
    pub instantiate: Option<unsafe extern "C" fn(*const Descriptor, c_ulong) -> Handle>,
    pub connect_port: Option<unsafe extern "C" fn(Handle, c_ulong, *mut f32)>,
    pub activate: Option<unsafe extern "C" fn(Handle)>,
    pub run: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub run_adding: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub set_run_adding_gain: Option<unsafe extern "C" fn(Handle, f32)>,
    pub deactivate: Option<unsafe extern "C" fn(Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(Handle)>,
}
// All pointers refer to immutable static storage and are never modified.
unsafe impl Sync for Descriptor {}
struct Names([*const c_char; PORTS]);
unsafe impl Sync for Names {}
static NAMES: Names = Names([
    c"Input L".as_ptr(),
    c"Input R".as_ptr(),
    c"Output L".as_ptr(),
    c"Output R".as_ptr(),
    c"Vocals".as_ptr(),
    c"Drums".as_ptr(),
    c"Bass".as_ptr(),
    c"Other".as_ptr(),
    c"Enabled".as_ptr(),
    c"State".as_ptr(),
    c"latency".as_ptr(),
]);
static TYPES: [c_int; PORTS] = [9, 9, 10, 10, 5, 5, 5, 5, 5, 6, 6];
const fn hint(descriptor: c_int, lower: f32, upper: f32) -> PortHint {
    PortHint {
        descriptor,
        lower,
        upper,
    }
}
static HINTS: [PortHint; PORTS] = [
    hint(0, 0.0, 0.0),
    hint(0, 0.0, 0.0),
    hint(0, 0.0, 0.0),
    hint(0, 0.0, 0.0),
    hint(3 | 0x100, -6.0, 6.0),
    hint(3 | 0x200, -6.0, 6.0),
    hint(3 | 0x200, -6.0, 6.0),
    hint(3 | 0x80, -6.0, 6.0),
    hint(4 | 0x240, 0.0, 1.0),
    hint(3 | 0x20, 0.0, 8.0),
    hint(3 | 0x20, 0.0, LATENCY as f32),
];
static DESCRIPTOR: Descriptor = Descriptor {
    unique_id: 0x14_5201,
    label: c"id14_remix_stereo".as_ptr(),
    properties: 0,
    name: c"iD14 stereo remix-v1".as_ptr(),
    maker: c"id14-remix".as_ptr(),
    copyright: c"MIT".as_ptr(),
    port_count: PORTS as c_ulong,
    port_descriptors: TYPES.as_ptr(),
    port_names: NAMES.0.as_ptr(),
    port_range_hints: HINTS.as_ptr(),
    implementation_data: std::ptr::null_mut(),
    instantiate: Some(instantiate),
    connect_port: Some(connect),
    activate: Some(activate),
    run: Some(run),
    run_adding: None,
    set_run_adding_gain: None,
    deactivate: Some(deactivate),
    cleanup: Some(cleanup),
};
struct Instance {
    engine: Engine,
    ports: [*mut f32; PORTS],
    rate: usize,
    path: Option<PathBuf>,
    has_run: bool,
    telemetry: Arc<Telemetry>,
    _publication: Publisher,
}
fn make_engine(rate: usize, path: Option<PathBuf>, telemetry: Arc<Telemetry>) -> Engine {
    Engine::with_telemetry(
        move || {
            if rate != RATE {
                return Err(Status::UnsupportedRate);
            }
            let path = path.ok_or(Status::ModelMissing)?;
            Ok(Box::new(model::OnnxModel::load(&path)?))
        },
        telemetry,
    )
}
unsafe extern "C" fn instantiate(_: *const Descriptor, rate: c_ulong) -> Handle {
    std::panic::catch_unwind(|| {
        let path = model::default_path();
        let telemetry = Arc::new(Telemetry::new());
        let engine = make_engine(rate as usize, path.clone(), telemetry.clone());
        let publication = Publisher::start(telemetry.clone(), path.clone());
        Box::into_raw(Box::new(Instance {
            engine,
            ports: [std::ptr::null_mut(); PORTS],
            rate: rate as usize,
            path,
            has_run: false,
            telemetry,
            _publication: publication,
        })) as Handle
    })
    .unwrap_or(std::ptr::null_mut())
}
unsafe extern "C" fn connect(handle: Handle, port: c_ulong, data: *mut f32) {
    if let Some(instance) = (handle as *mut Instance).as_mut() {
        if let Some(slot) = instance.ports.get_mut(port as usize) {
            *slot = data;
        }
    }
}
unsafe extern "C" fn activate(handle: Handle) {
    if let Some(instance) = (handle as *mut Instance).as_mut() {
        if instance.has_run {
            instance.engine.stop_worker();
            instance.engine = make_engine(
                instance.rate,
                instance.path.clone(),
                instance.telemetry.clone(),
            );
        }
        instance.has_run = false;
    }
}
unsafe extern "C" fn run(handle: Handle, count: c_ulong) {
    let Some(instance) = (handle as *mut Instance).as_mut() else {
        return;
    };
    let ports = instance.ports;
    let defaults = Controls::default();
    let control = |port: usize, default: f32| {
        if ports[port].is_null() {
            default
        } else {
            *ports[port]
        }
    };
    instance.engine.set_controls(Controls {
        enabled: control(8, 1.0) > 0.0,
        db: std::array::from_fn(|i| control(i + 4, defaults.db[i])),
    });
    for n in 0..count as usize {
        // Read both before either write: ordinary stereo in-place processing is supported.
        let input = std::array::from_fn(|ch| {
            if ports[ch].is_null() {
                0.0
            } else {
                *ports[ch].add(n)
            }
        });
        let output = instance.engine.sample(input);
        for ch in 0..2 {
            if !ports[ch + 2].is_null() {
                *ports[ch + 2].add(n) = output[ch];
            }
        }
    }
    if !ports[9].is_null() {
        *ports[9] = instance.engine.status() as u32 as f32;
    }
    if !ports[10].is_null() {
        *ports[10] = instance.engine.latency_frames() as f32;
    }
    instance.has_run = true;
}
unsafe extern "C" fn deactivate(_: Handle) {}
unsafe extern "C" fn cleanup(handle: Handle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle as *mut Instance));
    }
}

/// # Safety
/// Returned descriptor and all referenced arrays are immutable static storage.
#[no_mangle]
pub extern "C" fn ladspa_descriptor(index: c_ulong) -> *const Descriptor {
    if index == 0 {
        &DESCRIPTOR
    } else {
        std::ptr::null()
    }
}

/// Diagnostic injection: terminate/join the inference worker; call outside run.
/// # Safety
/// `handle` must be a live instance, exclusively owned by the calling host.
#[no_mangle]
pub unsafe extern "C" fn id14_remix_stop_worker(handle: Handle) {
    if let Some(instance) = (handle as *mut Instance).as_mut() {
        instance.engine.stop_worker();
    }
}
