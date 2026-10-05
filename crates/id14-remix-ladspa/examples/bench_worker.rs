//! Release-only reproduction of the exact worker adapter plus tract node profiling.
use id14_remix_ladspa::{dsp::BINS, engine::SeparationModel, model::OnnxModel};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Instant};
use tract_onnx::{
    prelude::*,
    tract_core::plan::{self, SimpleState},
};

fn stats(name: &str, samples: &mut [f64]) {
    samples.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"phase":name,"samples":samples.len(),
        "p50_ms":samples[samples.len()/2],"p99_ms":samples[((samples.len()-1) as f64*0.99).ceil() as usize],
        "max_ms":samples[samples.len()-1]})
    );
}
fn elapsed(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
fn stage(name: &str, start: Instant) {
    println!(
        "{}",
        serde_json::json!({"load_stage":name,"ms":elapsed(start)})
    );
}
fn main() -> TractResult<()> {
    let path = std::env::args().nth(1).expect("model path");
    let count: usize = std::env::args().nth(2).unwrap_or("300".into()).parse()?;
    let mut samples = Vec::with_capacity(count);
    // Execute the actual adapter on a dedicated worker, as the plugin does.
    let worker_path = path.clone();
    let measurements = std::thread::spawn(move || {
        let now = Instant::now();
        let mut model = OnnxModel::load(Path::new(&worker_path)).expect("valid remix-v1 model");
        stage("actual_adapter_load_and_probe", now);
        println!(
            "{}",
            serde_json::json!({"adapter_load_stages_ms":{
            "decode_validate":model.load_ms[0],"runtime_init":model.load_ms[1],
            "session_optimize_build":model.load_ms[2],"probe":model.load_ms[3]}})
        );
        let mut x = [0.0; 4 * BINS];
        let mut timings = Vec::new();
        for hop in 0..count + 20 {
            for (i, v) in x.iter_mut().enumerate() {
                *v = ((i * 17 + hop * 13) as f32 * 0.013).sin() * 0.1;
            }
            let start = Instant::now();
            std::hint::black_box(model.infer(&x).expect("inference"));
            if hop >= 20 {
                timings.push(elapsed(start));
            }
        }
        let mut phases = [Vec::new(), Vec::new(), Vec::new()];
        for _ in 0..count {
            let (_, times) = model.infer_profiled(&x).unwrap();
            for (v, t) in phases.iter_mut().zip(times) {
                v.push(t);
            }
        }
        for (label, values) in [
            "adapter_input_copy",
            "adapter_session_run",
            "adapter_output_copy_validate",
        ]
        .into_iter()
        .zip(&mut phases)
        {
            stats(label, values);
        }
        timings
    })
    .join()
    .unwrap();
    stats("actual_worker_infer", &mut measurements.clone());
    let mut sorted = measurements.clone();
    sorted.sort_by(f64::total_cmp);
    assert!(
        sorted[((sorted.len() - 1) as f64 * 0.99).ceil() as usize] <= 5.0,
        "worker p99 exceeds 5 ms"
    );
    if !std::env::args().any(|s| s == "--tract-profile") {
        return Ok(());
    }

    let now = Instant::now();
    let bytes = std::fs::read(&path)?;
    stage("file_read", now);
    let onnx = tract_onnx::onnx();
    let now = Instant::now();
    let proto = onnx.proto_model_for_read(&mut std::io::Cursor::new(bytes))?;
    stage("protobuf_decode", now);
    let now = Instant::now();
    let model = onnx.model_for_proto_model(&proto)?;
    stage("onnx_import", now);
    let now = Instant::now();
    let model = model.into_optimized()?;
    stage("into_optimized", now);
    let now = Instant::now();
    let runnable = Arc::new(model.into_runnable()?);
    stage("into_runnable", now);
    let s = runnable.model().input_fact(1)?.shape.as_concrete().unwrap()[1];
    let mut input_times = Vec::new();
    let mut init_times = Vec::new();
    let mut output_times = Vec::new();
    let mut ops = BTreeMap::<String, f64>::new();
    let mut state = vec![0.0; s];
    let features = [0.01_f32; 4 * BINS];
    for _ in 0..50 {
        let now = Instant::now();
        let input = tvec!(
            Tensor::from_shape(&[1, 4, BINS], &features)?.into(),
            Tensor::from_shape(&[1, s], &state)?.into()
        );
        input_times.push(elapsed(now));
        let now = Instant::now();
        let mut session = SimpleState::new(runnable.clone())?;
        init_times.push(elapsed(now));
        let start = Instant::now();
        let output = session.run_plan_with_eval(input, |session, op, node, inputs| {
            let start = Instant::now();
            let result = plan::eval(session, op, node, inputs);
            let ms = elapsed(start);
            *ops.entry(format!("{} :: {}", node.name, node.op.name()))
                .or_default() += ms;
            result
        })?;
        samples.push(elapsed(start));
        let now = Instant::now();
        let mut mask = [0.0; 4 * BINS];
        mask.copy_from_slice(output[0].as_slice::<f32>()?);
        state.copy_from_slice(output[1].as_slice::<f32>()?);
        assert!(id14_remix_ladspa::dsp::valid_mask(&mask));
        output_times.push(elapsed(now));
    }
    stats("input_alloc_copy", &mut input_times);
    stats("execution_state_new", &mut init_times);
    stats("profiled_execution", &mut samples);
    stats("output_copy_validate", &mut output_times);
    let mut groups = BTreeMap::<String, f64>::new();
    for (name, total) in &ops {
        let group = if name.to_lowercase().contains("gru") {
            "GRU"
        } else {
            name.rsplit(" :: ").next().unwrap()
        };
        *groups.entry(group.into()).or_default() += total;
    }
    for (name, total) in groups {
        println!(
            "{}",
            serde_json::json!({"operator_group":name,"mean_ms":total/50.0})
        );
    }
    let mut ops: Vec<_> = ops.into_iter().collect();
    ops.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (name, total) in ops.into_iter().take(15) {
        println!("{}", serde_json::json!({"node":name,"mean_ms":total/50.0}));
    }
    // A/B: retain tract's execution scratch, while passing explicit ONNX state.
    let mut session = SimpleState::new(runnable.clone())?;
    let mut timings = Vec::new();
    for i in 0..count + 20 {
        let now = Instant::now();
        let output = session.run(tvec!(
            Tensor::from_shape(&[1, 4, BINS], &features)?.into(),
            Tensor::from_shape(&[1, s], &state)?.into()
        ))?;
        state.copy_from_slice(output[1].as_slice::<f32>()?);
        if i >= 20 {
            timings.push(elapsed(now));
        }
    }
    stats("reused_execution_state", &mut timings);
    let mut fast = OnnxModel::load(Path::new(&path)).unwrap();
    state.fill(0.0);
    let mut error = 0.0_f32;
    for hop in 0..32 {
        let x = std::array::from_fn(|i| ((i * 17 + hop * 13) as f32 * 0.013).sin() * 0.1);
        let actual = fast.infer(&x).unwrap();
        let output = runnable.run(tvec!(
            Tensor::from_shape(&[1, 4, BINS], &x)?.into(),
            Tensor::from_shape(&[1, s], &state)?.into()
        ))?;
        if hop == 0 {
            if let Ok(path) = std::env::var("ID14_BENCH_OUTPUT") {
                std::fs::write(
                    path,
                    serde_json::to_vec(&serde_json::json!({
                        "x": x.to_vec(), "state": vec![0.0_f32;s],
                        "ort": actual.to_vec(), "tract": output[0].as_slice::<f32>()?
                    }))?,
                )?;
            }
        }
        state.copy_from_slice(output[1].as_slice::<f32>()?);
        let hop_error = actual
            .iter()
            .zip(output[0].as_slice::<f32>()?)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        error = error.max(hop_error);
        if hop < 3 {
            println!(
                "{}",
                serde_json::json!({"parity_hop":hop,"max_absolute_error":hop_error})
            );
        }
    }
    println!(
        "{}",
        serde_json::json!({"check":"ort_tract_mask_parity","hops":32,"max_absolute_error":error})
    );
    // Informational comparison, not an oracle: the ONNX reference evaluator
    // agrees with ORT (2.98e-7 on the first hop), while tract differs by 0.0636.
    // Keep the independent reference gate in tools/verify_onnx_reference.py.
    Ok(())
}
