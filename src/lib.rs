use wasm_bindgen::prelude::*;


mod gpu {
    use wgpu_compute::import_wgpu_compute;

    import_wgpu_compute! {
        file: "./shader.wgsl",
    }

    //pub use double;
}


async fn double(values: Vec<f32>) -> Vec<f32> {
    let threads = values.len();

    let mut state = wgpu_compute::State::new(gpu::Bindings {
        output: vec![0.0; values.len()],
        input: values,
    }).await;

    let output = state.output(|bindings| &bindings.output);

    gpu::double(state, threads).await;

    output.to_vec()
}


#[wasm_bindgen(start)]
pub async fn main_js() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap();

    log::info!("STARTING");

    let output = double(vec![0.0, 2.0, 6.0, 10.0, 30.0, 60.0, 100.0]).await;

    log::info!("{:?}", output);

    Ok(())
}
