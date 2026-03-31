struct Input {
    value: f32,
}

struct Output {
    value: f32,
}

@group(0) @binding(0)
var<storage, read> input: array<Input>;

@group(0) @binding(1)
var<storage, read_write> output: array<Output>;

const FOO: Input = Input(3.0);

const BAR: vec2<f32> = vec2<f32>(5.0, 1.0);

const QUX: mat2x3f = mat2x3f(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);

fn double_(value: f32) -> f32 {
    return value * 2.0;
}

@compute @workgroup_size(64)
fn double(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;

    if (index >= arrayLength(&input)) {
        return;
    }

    output[index].value = double_(input[index].value);
}
