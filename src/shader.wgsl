@group(0) @binding(0)
var<storage, read> input: array<f32>;

@group(0) @binding(1)
var<storage, read_write> output: array<f32>;

fn double_(input: f32) -> f32 {
    return input * 2.0;
}

@compute @workgroup_size(64)
fn double(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;

    if (index >= arrayLength(&input)) {
        return;
    }

    output[index] = double_(input[index]);
}
