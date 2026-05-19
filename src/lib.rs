use wasm_bindgen::prelude::*;
use bytemuck::Zeroable;


mod gpu {
    wgpu_compute::import_wgpu_compute! {
        file: "./shader.wgsl",
    }
}


async fn double(values: Vec<gpu::Input>) -> Vec<gpu::Output> {
    let threads = values.len() as u32;

    let mut state = wgpu_compute::State::new(gpu::Bindings {
        output: vec![Zeroable::zeroed(); values.len()],
        input: values,
    }).await;

    let output = state.bindings().output.read();

    gpu::double(state, threads).await;

    output.to_vec()
}


#[wasm_bindgen(start)]
pub async fn main_js() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap();

    log::info!("STARTING");

    let output = double(vec![0.0, 2.0, 6.0, 10.0, 30.0, 60.0, 100.0].into_iter().map(|value| gpu::Input { value }).collect()).await;

    log::info!("{:?}", output);

    Ok(())
}
