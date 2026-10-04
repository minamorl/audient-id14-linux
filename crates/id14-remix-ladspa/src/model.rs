//! ONNX adapter. All calls, allocations, filesystem access and diagnostics occur on the worker.
use crate::dsp::{valid_mask, BINS, MAX_LOOKAHEAD};
use crate::engine::{SeparationModel, Status};
use std::path::{Path, PathBuf};
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
    plan: TypedRunnableModel<TypedModel>,
    q: usize,
    state: Vec<f32>,
    x_index: usize,
    mask_index: usize,
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
        let plan = onnx
            .model_for_proto_model(&proto)
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|_| Status::ModelInvalid)?;
        let mut model = Self {
            plan,
            q,
            state,
            x_index,
            mask_index,
        };
        // Reject malformed concrete outputs before announcing the model as ready.
        model.infer(&[0.0; 4 * BINS])?;
        model.reset();
        Ok(model)
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
        let features = Tensor::from_shape(&[1, 4, BINS], x).map_err(|_| Status::ModelInvalid)?;
        let state = Tensor::from_shape(&[1, self.state.len()], &self.state)
            .map_err(|_| Status::ModelInvalid)?;
        let input = if self.x_index == 0 {
            tvec!(features.into(), state.into())
        } else {
            tvec!(state.into(), features.into())
        };
        let result = self.plan.run(input).map_err(|_| Status::ModelInvalid)?;
        if result.len() != 2
            || result[self.mask_index].shape() != [1, 4, BINS]
            || result[1 - self.mask_index].shape() != [1, self.state.len()]
        {
            return Err(Status::ModelInvalid);
        }
        let mask_view = result[self.mask_index]
            .as_slice::<f32>()
            .map_err(|_| Status::ModelInvalid)?;
        let mut mask = [0.0; 4 * BINS];
        mask.copy_from_slice(mask_view);
        let state_view = result[1 - self.mask_index]
            .as_slice::<f32>()
            .map_err(|_| Status::ModelInvalid)?;
        if !valid_mask(&mask) || state_view.iter().any(|x| !x.is_finite()) {
            return Err(Status::ModelInvalid);
        }
        self.state.copy_from_slice(state_view);
        Ok(mask)
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
        for name in ["voice", "uniform", "voice_bins", "q3", "stateful_reordered"] {
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
