//! ONNX adapter. All calls, allocations, filesystem access and diagnostics occur on the worker.
use crate::dsp::{valid_mask, BINS, MAX_LOOKAHEAD};
use crate::engine::{SeparationModel, Status};
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor as OrtTensor,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tract_onnx::{pb, prelude::*};

pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("ID14_REMIX_MODEL")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".local/share/id14-sr/remix.onnx"))
        })
}

fn failure(code: Status, reason: &str) -> Status {
    let trace_id = uuid::Uuid::now_v7().to_string();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    eprintln!(
        "{}",
        serde_json::json!({
            "ts": ts, "level": "warn", "trace_id": trace_id,
            "msg": "Remix model unavailable; continuing with neutral playback",
            "error": {"code": format!("{code:?}"), "message": "Remix model unavailable",
                      "details": reason, "trace_id": trace_id}
        })
    );
    code
}

pub struct OnnxModel {
    session: Session,
    features: OrtTensor<f32>,
    recurrent: OrtTensor<f32>,
    q: usize,
    state: Vec<f32>,
    /// Decode/validate, runtime init, session optimize/build, initial probe (ms).
    pub load_ms: [f64; 4],
}

fn load_runtime(path: &Path) -> Result<(libloading::Library, ort::sys::OrtApi), Status> {
    // Avoid ort rc.12's bootstrap error path: constructing ort::Error there
    // itself calls the not-yet-loaded native API. No ort call before set_api.
    // SAFETY: the runtime is selected by the host environment/installation. Its
    // documented entrypoint and versioned C API are checked before use. The
    // returned handle must outlive the copied function table.
    let library = unsafe { libloading::Library::new(path) }.map_err(|_| Status::ModelInvalid)?;
    let api = unsafe {
        let get: libloading::Symbol<unsafe extern "system" fn() -> *const ort::sys::OrtApiBase> =
            library
                .get(b"OrtGetApiBase\0")
                .map_err(|_| Status::ModelInvalid)?;
        let base = get().as_ref().ok_or(Status::ModelInvalid)?;
        let version_ptr = (base.GetVersionString)();
        if version_ptr.is_null() {
            return Err(Status::ModelInvalid);
        }
        let version = std::ffi::CStr::from_ptr(version_ptr)
            .to_str()
            .map_err(|_| Status::ModelInvalid)?;
        let minor = version
            .split('.')
            .nth(1)
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or(Status::ModelInvalid)?;
        if minor < ort::sys::ORT_API_VERSION {
            return Err(Status::ModelInvalid);
        }
        let api = (base.GetApi)(ort::sys::ORT_API_VERSION);
        if api.is_null() {
            return Err(Status::ModelInvalid);
        }
        std::ptr::read(api)
    };
    Ok((library, api))
}

fn runtime() -> Result<(), Status> {
    // The C API contains function pointers into this library. Keep the handle
    // alive for the process lifetime, including all sessions and error objects.
    static READY: OnceLock<Result<libloading::Library, Status>> = OnceLock::new();
    READY
        .get_or_init(|| {
            // Resolve only at runtime: the installer retains the HOME path with
            // a Nix GC root. A build-machine store path must never be embedded.
            let candidates = [
                std::env::var_os("ID14_ORT_LIBRARY").map(PathBuf::from),
                std::env::var_os("HOME").map(|home| {
                    PathBuf::from(home).join(".local/lib/id14-sr/onnxruntime/lib/libonnxruntime.so")
                }),
                Some(PathBuf::from("libonnxruntime.so")),
            ];
            let (library, api) = candidates
                .iter()
                .flatten()
                .find_map(|path| load_runtime(path).ok())
                .ok_or(Status::ModelInvalid)?;
            // Rejected candidates never install a global API. READY keeps this
            // accepted library alive for all sessions and native error objects.
            if !ort::set_api(api) {
                return Err(Status::ModelInvalid);
            }
            ort::init()
                .with_name("id14-remix")
                .with_logger(Arc::new(|_, _, _, _, _| {}))
                .commit();
            Ok(library)
        })
        .as_ref()
        .map(|_| ())
        .map_err(|status| *status)
}

fn shape(info: &pb::ValueInfoProto) -> Result<Vec<usize>, Status> {
    let Some(pb::type_proto::Value::TensorType(tensor)) =
        info.r#type.as_ref().and_then(|t| t.value.as_ref())
    else {
        return Err(Status::ModelInvalid);
    };
    if tensor.elem_type != pb::tensor_proto::DataType::Float as i32 {
        return Err(Status::ModelInvalid);
    }
    tensor
        .shape
        .as_ref()
        .ok_or(Status::ModelInvalid)?
        .dim
        .iter()
        .map(|d| match d.value.as_ref() {
            Some(pb::tensor_shape_proto::dimension::Value::DimValue(n)) if *n >= 0 => {
                usize::try_from(*n).map_err(|_| Status::ModelInvalid)
            }
            _ => Err(Status::ModelInvalid),
        })
        .collect()
}

fn embedded_tensor(t: &pb::TensorProto) -> bool {
    t.external_data.is_empty() && t.data_location.unwrap_or(0) == 0
}
fn embedded_graph(g: &pb::GraphProto) -> bool {
    g.initializer.iter().all(embedded_tensor)
        && g.node.iter().all(|n| {
            n.attribute.iter().all(|a| {
                a.t.as_ref().is_none_or(embedded_tensor)
                    && a.tensors.iter().all(embedded_tensor)
                    && a.g.as_ref().is_none_or(embedded_graph)
                    && a.graphs.iter().all(embedded_graph)
            })
        })
}

impl OnnxModel {
    pub fn load(path: &Path) -> Result<Self, Status> {
        let data = std::fs::read(path)
            .map_err(|_| failure(Status::ModelMissing, "file is missing or unreadable"))?;
        Self::from_bytes(&data).map_err(|status| {
            failure(
                status,
                "remix-v1 signature, metadata, operators or outputs are invalid",
            )
        })
    }
    pub fn from_bytes(data: &[u8]) -> Result<Self, Status> {
        let started = Instant::now();
        let onnx = tract_onnx::onnx();
        let proto = onnx
            .proto_model_for_read(&mut std::io::Cursor::new(data))
            .map_err(|_| Status::ModelInvalid)?;
        let metadata = |key: &str| -> Result<&str, Status> {
            let mut matches = proto.metadata_props.iter().filter(|p| p.key == key);
            let value = &matches.next().ok_or(Status::ModelInvalid)?.value;
            if matches.next().is_some() {
                return Err(Status::ModelInvalid);
            }
            Ok(value)
        };
        if metadata("id14.contract")? != "remix-v1" {
            return Err(Status::ModelInvalid);
        }
        let q_text = metadata("id14.lookahead_frames")?;
        if q_text.is_empty() || !q_text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Status::ModelInvalid);
        }
        let q: usize = q_text.parse().map_err(|_| Status::ModelInvalid)?;
        if q > MAX_LOOKAHEAD {
            return Err(Status::ModelInvalid);
        }
        let graph = proto.graph.as_ref().ok_or(Status::ModelInvalid)?;
        if graph.input.len() != 2 || graph.output.len() != 2 || !embedded_graph(graph) {
            return Err(Status::ModelInvalid);
        }
        if graph
            .initializer
            .iter()
            .any(|t| t.name == "x" || t.name == "state")
        {
            return Err(Status::ModelInvalid);
        }
        let x_index = graph
            .input
            .iter()
            .position(|i| i.name == "x")
            .ok_or(Status::ModelInvalid)?;
        let s_index = graph
            .input
            .iter()
            .position(|i| i.name == "state")
            .ok_or(Status::ModelInvalid)?;
        let mask_index = graph
            .output
            .iter()
            .position(|i| i.name == "mask")
            .ok_or(Status::ModelInvalid)?;
        let out_index = graph
            .output
            .iter()
            .position(|i| i.name == "state_out")
            .ok_or(Status::ModelInvalid)?;
        if shape(&graph.input[x_index])? != [1, 4, BINS]
            || shape(&graph.output[mask_index])? != [1, 4, BINS]
        {
            return Err(Status::ModelInvalid);
        }
        let state_shape = shape(&graph.input[s_index])?;
        if state_shape.len() != 2
            || state_shape[0] != 1
            || shape(&graph.output[out_index])? != state_shape
        {
            return Err(Status::ModelInvalid);
        }
        let state = vec![0.0; state_shape[1]];
        let validation_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        runtime()?;
        let runtime_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let session = (|| -> ort::Result<Session> {
            Session::builder()?
                .with_no_environment_execution_providers()?
                .with_intra_threads(1)?
                .with_inter_threads(1)?
                .with_parallel_execution(false)?
                .with_intra_op_spinning(false)?
                .with_inter_op_spinning(false)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .commit_from_memory(data)
        })()
        .map_err(|_| Status::ModelInvalid)?;
        let features = OrtTensor::<f32>::new(session.allocator(), [1, 4, BINS])
            .map_err(|_| Status::ModelInvalid)?;
        let recurrent = OrtTensor::<f32>::new(session.allocator(), [1, state.len()])
            .map_err(|_| Status::ModelInvalid)?;
        let session_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut model = Self {
            session,
            features,
            recurrent,
            q,
            state,
            load_ms: [validation_ms, runtime_ms, session_ms, 0.0],
        };
        // Reject malformed concrete outputs before announcing the model as ready.
        let started = Instant::now();
        model.infer(&[0.0; 4 * BINS])?;
        model.load_ms[3] = started.elapsed().as_secs_f64() * 1000.0;
        model.reset();
        Ok(model)
    }
    /// Same inference path, with optional phase clocks for the worker benchmark.
    pub fn infer_profiled(
        &mut self,
        x: &[f32; 4 * BINS],
    ) -> Result<([f32; 4 * BINS], [f64; 3]), Status> {
        self.infer_inner::<true>(x)
    }
    fn infer_inner<const PROFILE: bool>(
        &mut self,
        x: &[f32; 4 * BINS],
    ) -> Result<([f32; 4 * BINS], [f64; 3]), Status> {
        let start = PROFILE.then(Instant::now);
        self.features
            .try_extract_tensor_mut()
            .map_err(|_| Status::ModelInvalid)?
            .1
            .copy_from_slice(x);
        self.recurrent
            .try_extract_tensor_mut()
            .map_err(|_| Status::ModelInvalid)?
            .1
            .copy_from_slice(&self.state);
        let input_ms = start
            .map(|s| s.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        let start = PROFILE.then(Instant::now);
        let result = self
            .session
            .run(ort::inputs!["x" => &self.features, "state" => &self.recurrent])
            .map_err(|_| Status::ModelInvalid)?;
        let run_ms = start
            .map(|s| s.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        let start = PROFILE.then(Instant::now);
        if result.len() != 2 {
            return Err(Status::ModelInvalid);
        }
        let (mask_shape, mask_view) = result["mask"]
            .try_extract_tensor::<f32>()
            .map_err(|_| Status::ModelInvalid)?;
        let (state_shape, state_view) = result["state_out"]
            .try_extract_tensor::<f32>()
            .map_err(|_| Status::ModelInvalid)?;
        if mask_shape.as_ref() != [1, 4, BINS as i64]
            || state_shape.as_ref() != [1, self.state.len() as i64]
        {
            return Err(Status::ModelInvalid);
        }
        let mut mask = [0.0; 4 * BINS];
        mask.copy_from_slice(mask_view);
        if !valid_mask(&mask) || state_view.iter().any(|x| !x.is_finite()) {
            return Err(Status::ModelInvalid);
        }
        self.state.copy_from_slice(state_view);
        let output_ms = start
            .map(|s| s.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        Ok((mask, [input_ms, run_ms, output_ms]))
    }
}
impl SeparationModel for OnnxModel {
    fn lookahead(&self) -> usize {
        self.q
    }
    fn reset(&mut self) {
        self.state.fill(0.0);
    }
    fn infer(&mut self, x: &[f32; 4 * BINS]) -> Result<[f32; 4 * BINS], Status> {
        self.infer_inner::<false>(x).map(|v| v.0)
    }
}

pub fn load_default() -> Result<Box<dyn SeparationModel>, Status> {
    let path = default_path().ok_or_else(|| failure(Status::ModelMissing, "HOME is unset"))?;
    Ok(Box::new(OnnxModel::load(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(format!("{name}.onnx")),
        )
        .unwrap()
    }
    #[test]
    fn accepts_valid_models_and_maps_reordered_names() {
        for name in [
            "voice",
            "uniform",
            "voice_bins",
            "q3",
            "stateful_reordered",
            "zero_state",
        ] {
            let mut model = OnnxModel::from_bytes(&fixture(name)).unwrap();
            let output = model.infer(&[0.1; 4 * BINS]).unwrap();
            assert!(valid_mask(&output));
            if name == "stateful_reordered" {
                assert_eq!(model.state, [1.0; 3]);
                model.infer(&[0.1; 4 * BINS]).unwrap();
                assert_eq!(model.state, [2.0; 3]);
                model.reset();
                assert_eq!(model.state, [0.0; 3]);
            }
            println!(
                "onnx_valid={name} q={} state_size={}",
                model.q,
                model.state.len()
            );
        }
    }
    #[test]
    fn rejects_contract_and_runtime_value_errors() {
        for name in [
            "bad_contract",
            "bad_q",
            "unsupported_q",
            "bad_shape",
            "bad_sum",
            "negative",
            "nan_mask",
            "nan_state",
        ] {
            assert!(
                matches!(
                    OnnxModel::from_bytes(&fixture(name)),
                    Err(Status::ModelInvalid)
                ),
                "{name}"
            );
            println!("onnx_rejected={name}");
        }
        assert!(matches!(
            OnnxModel::from_bytes(b"not onnx"),
            Err(Status::ModelInvalid)
        ));
    }
}
