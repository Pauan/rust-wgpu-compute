use std::cell::RefCell;
use bytemuck::NoUninit;

pub use wgpu_compute_macro::import_wgpu_compute;

mod util;
//mod buffer;

//pub use buffer::*;


#[derive(Clone)]
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    fn load() -> impl Future<Output = Option<Self>> + use<> {
        async move {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

            if let Ok(adapter) = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await {
                log::debug!("Found GPU adapter: {:#?}", adapter.get_info());

                let downlevel_capabilities = adapter.get_downlevel_capabilities();

                // If we have GPU compute capabilities...
                if downlevel_capabilities.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS) {
                    let request_device = adapter.request_device(&wgpu::DeviceDescriptor {
                        label: None,
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::downlevel_defaults(),
                        experimental_features: wgpu::ExperimentalFeatures::disabled(),
                        memory_hints: wgpu::MemoryHints::MemoryUsage,
                        trace: wgpu::Trace::Off,
                    });

                    if let Ok((device, queue)) = request_device.await {
                        return Some(Self { device, queue });
                    }
                }
            }

            log::debug!("Could not initialize GPU compute, falling back to CPU");
            None
        }
    }


    fn get() -> impl Future<Output = Option<Self>> + use<> {
        thread_local! {
            static GPU: RefCell<util::Loading<Option<Gpu>>> = RefCell::new(util::Loading::Init);
        }

        async move {
            let maybe_init = GPU.with(|gpu| {
                // TODO properly cleanup if this gets canceled
                gpu.borrow_mut().init(|| async move {
                    let loaded = Self::load().await;

                    GPU.with(|gpu| {
                        gpu.borrow_mut().done(loaded);
                    });
                })
            });

            if let Some(future) = maybe_init {
                future.await;

            } else {
                // TODO properly cleanup if this gets canceled
                let receiver = GPU.with(|gpu| gpu.borrow_mut().wait());

                if let Some(receiver) = receiver {
                    let _ = receiver.await;
                }
            }

            GPU.with(|gpu| gpu.borrow().value())
        }
    }


    fn input_buffer(&self, value: &[u8]) -> wgpu::Buffer {
        // Buffer which contains the input data, which will be sent to the GPU.
        let input_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: value.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });

        input_buffer.slice(..)
            .get_mapped_range_mut()[..]
            .copy_from_slice(value);

        input_buffer.unmap();

        input_buffer
    }

    pub fn input<A>(&self, value: &A) -> Input<A> where A: NoUninit {
        Input {
            buffer: Buffer::Gpu(self.input_buffer(bytemuck::bytes_of(value))),
        }
    }

    pub fn input_vec<A>(&self, value: &[A]) -> Input<Vec<A>> where A: NoUninit {
        Input {
            buffer: Buffer::Gpu(self.input_buffer(bytemuck::cast_slice(value))),
        }
    }

    pub fn bind_group<'a, A>(input: &'a Input<A>) -> wgpu::BindingResource<'a> {
        match &input.buffer {
            Buffer::Gpu(buffer) => {
                buffer.as_entire_binding()
            },
            Buffer::Cpu(_) => {
                panic!("Cannot create bind group from CPU Buffer");
            },
        }
    }


    fn output_buffer(&self, input_buffer: &wgpu::Buffer) -> wgpu::Buffer {
        // We have to use a separate buffer for transferring from the GPU to the CPU,
        // because the browser does not allow for combining `MAP_READ` with `STORAGE`.
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: input_buffer.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }

    pub fn output<A>(&self, input: &Input<A>) -> Output<A> {
        let (_, output) = self.input_output(input);

        Output {
            buffer: Buffer::Gpu(output),
        }
    }


    fn input_output<'a, A>(&self, input: &'a Input<A>) -> (&'a wgpu::Buffer, wgpu::Buffer) {
        match &input.buffer {
            Buffer::Gpu(buffer) => {
                (buffer, self.output_buffer(buffer))
            },
            Buffer::Cpu(_) => {
                panic!("Cannot create a GPU Output from a CPU Input");
            },
        }
    }
}


#[allow(unused)]
enum Buffer<A> {
    Gpu(wgpu::Buffer),
    Cpu(A),
}


pub struct Input<A> {
    buffer: Buffer<A>,
}


pub struct Output<A> {
    buffer: Buffer<Option<A>>,
}

/*impl<A> Output<A> {
    fn buffer(&self) -> &wgpu::Buffer {
        match &self.buffer {
            Buffer::Gpu(buffer) => buffer,
            Buffer::Cpu(_) => unreachable!(),
        }
    }
}*/

impl<A> Output<A> where A: bytemuck::AnyBitPattern {
    pub fn value(&self) -> A {
        match self.buffer {
            Buffer::Gpu(ref buffer) => {
                let data = buffer.get_mapped_range(..);

                *bytemuck::from_bytes(&data)
            },
            Buffer::Cpu(ref value) => {
                *value.as_ref().expect("Output is empty")
            },
        }
    }
}

impl<A> Output<Vec<A>> where A: bytemuck::AnyBitPattern {
    pub fn to_vec(&self) -> Vec<A> {
        match self.buffer {
            Buffer::Gpu(ref buffer) => {
                let data = buffer.get_mapped_range(..);

                bytemuck::cast_slice(&data).to_vec()
            },
            Buffer::Cpu(ref value) => {
                value.as_ref().expect("Output is empty").to_vec()
            },
        }
    }
}


pub trait IntoBuffers {
    type Cpu;
    type Gpu;

    fn into_cpu_buffers(self) -> Self::Cpu;

    fn into_gpu_buffers(self, gpu: &Gpu) -> Self::Gpu;
}


#[repr(transparent)]
pub struct StateCpu<A>(__internal::StateCpu<A>);

impl<A> StateCpu<A> {
    pub fn new<B>(bindings: B) -> Self where B: IntoBuffers<Cpu = A> {
        Self(__internal::StateCpu {
            buffers: bindings.into_cpu_buffers(),
        })
    }
}

impl<A> Into<__internal::StateCpu<A>> for StateCpu<A> {
    #[inline]
    fn into(self) -> __internal::StateCpu<A> {
        self.0
    }
}


#[repr(transparent)]
pub struct StateGpu<A>(__internal::StateGpu<A>);

impl<A> StateGpu<A> {
    fn new_with_gpu(buffers: A, gpu: Gpu) -> Self {
        Self(__internal::StateGpu {
            buffers,
            copy_buffers: vec![],
            gpu,
        })
    }

    pub fn new<B>(bindings: B) -> impl Future<Output = Option<Self>> + use<A, B>
        where B: IntoBuffers<Gpu = A> {

        async move {
            match Gpu::get().await {
                Some(gpu) => Some(Self::new_with_gpu(bindings.into_gpu_buffers(&gpu), gpu)),
                None => None,
            }
        }
    }

    /*pub fn output<B, F>(&mut self, f: F) -> Output<B> where F: FnOnce(&A) -> &Input<B> {
        match &mut self.0 {
            __internal::State::Gpu(state) => {
                let (input, output) = state.gpu.input_output(f(&state.buffers));

                state.copy_buffers.push((input.clone(), output.clone()));

                Output {
                    buffer: Buffer::Gpu(output),
                }
            },

            __internal::State::Cpu(_cpu) => {
                todo!();
            },
        }
    }*/
}

impl<A> Into<__internal::StateGpu<A>> for StateGpu<A> {
    #[inline]
    fn into(self) -> __internal::StateGpu<A> {
        self.0
    }
}


pub enum State<A> where A: IntoBuffers {
    Cpu(StateCpu<A::Cpu>),
    Gpu(StateGpu<A::Gpu>),
}

impl<A> State<A> where A: IntoBuffers {
    pub fn new(bindings: A) -> impl Future<Output = Self> + use<A> {
        async move {
            match Gpu::get().await {
                Some(gpu) => Self::Gpu(StateGpu::new_with_gpu(bindings.into_gpu_buffers(&gpu), gpu)),
                None => Self::Cpu(StateCpu::new(bindings)),
            }
        }
    }
}


#[doc(hidden)]
pub mod __internal {
    use crate::util::make_empty_callback;

    use super::{Gpu};


    pub struct StateCpu<A> {
        pub buffers: A,
    }


    pub struct StateGpu<B> {
        pub buffers: B,
        pub gpu: Gpu,
        pub copy_buffers: Vec<(wgpu::Buffer, wgpu::Buffer)>,
    }


    pub struct GpuLayout {
        shader: wgpu::ShaderModule,
        bind_group_layouts: Vec<wgpu::BindGroupLayout>,
        pipeline_layout: wgpu::PipelineLayout,
    }

    impl GpuLayout {
        pub fn new(gpu: &Gpu, shader: &str, bind_groups: &[&[wgpu::BindGroupLayoutEntry]]) -> Self {
            let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });

            let bind_group_layouts = bind_groups.into_iter().map(|entries| {
                gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries,
                })
            }).collect::<Vec<_>>();

            let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: bind_group_layouts.iter().collect::<Vec<_>>().as_slice(),
                immediate_size: 0,
            });

            Self {
                shader,
                bind_group_layouts,
                pipeline_layout,
            }
        }
    }


    pub struct GpuFn {
        pipeline: wgpu::ComputePipeline,
        threads: u32,
    }

    impl GpuFn {
        pub fn new(gpu: &Gpu, layout: &GpuLayout, workgroups: [u32; 3], name: &str) -> Self {
            let threads = workgroups[0] * workgroups[1] * workgroups[2];

            let pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&layout.pipeline_layout),
                module: &layout.shader,
                entry_point: Some(name),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

            Self {
                pipeline,
                threads,
            }
        }
    }


    pub fn command_encoder(
        gpu: &Gpu,
        layout: &GpuLayout,
        gpu_fn: &GpuFn,
        threads: u32,
        bindings: &[&[wgpu::BindGroupEntry]],
    ) -> wgpu::CommandEncoder {
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });

            compute_pass.set_pipeline(&gpu_fn.pipeline);

            for (index, (layout, entries)) in layout.bind_group_layouts.iter().zip(bindings.into_iter()).enumerate() {
                let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout,
                    entries,
                });

                compute_pass.set_bind_group(index as u32, &bind_group, &[]);
            }

            // Splits the work into multiple workgroups based on the `threads`
            // e.g. if `threads` is 64, and the number of arguments is 320, then
            // it will dispatch 5 workgroups, and each workgroup will have 64 threads.
            let workgroup_count = threads.div_ceil(gpu_fn.threads);

            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        encoder
    }


    pub fn copy_input_to_output(encoder: &mut wgpu::CommandEncoder, input_buffer: &wgpu::Buffer, output_buffer: &wgpu::Buffer) {
        let size = input_buffer.size();

        // Transfers the data from the input_buffer into the output_buffer, so we can read the data.
        encoder.copy_buffer_to_buffer(
            &input_buffer,
            0,
            &output_buffer,
            0,
            size,
        );

        encoder.map_buffer_on_submit(&output_buffer, wgpu::MapMode::Read, .., |_| {});
    }


    pub fn wait(gpu: &Gpu, encoder: wgpu::CommandEncoder) -> impl Future<Output = ()> + use<> {
        let (callback, future) = make_empty_callback();

        encoder.on_submitted_work_done(callback);

        let command_buffer = encoder.finish();

        gpu.queue.submit([command_buffer]);

        // Wait for the queued commands to finish
        future
    }
}


/*
/// Utility for writing a `Vec<T>` into a `wgpu::Buffer`.
///
/// It will automatically resize the buffer to match the Vec's capacity.
#[repr(transparent)]
pub(crate) struct VecBuffer<T> {
    buffer: Option<wgpu::Buffer>,
    _phantom: PhantomData<Vec<T>>,
}

impl<T> VecBuffer<T> where T: bytemuck::Pod  {
    pub(crate) fn new() -> Self {
        Self {
            buffer: None,
            _phantom: PhantomData,
        }
    }

    fn byte_capacity(values: &Vec<T>) -> u64 {
        (values.capacity() * std::mem::size_of::<T>()) as u64
    }

    fn byte_len(values: &Vec<T>) -> u64 {
        (values.len() * std::mem::size_of::<T>()) as u64
    }

    /// This should only be called if vec_capacity > 0
    fn make_buffer<'a>(vec_capacity: u64, values: &Vec<T>, engine: &crate::EngineState, settings: VecBufferSettings<'a>) -> wgpu::Buffer {
        let vec_len = Self::byte_len(values);

        assert!(vec_capacity >= vec_len);

        let buffer = engine.device.create_buffer(&wgpu::BufferDescriptor {
            label: settings.label,
            size: vec_capacity,
            usage: settings.usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });

        if vec_len > 0 {
            buffer.slice(..vec_len)
                .get_mapped_range_mut()
                .copy_from_slice(bytemuck::cast_slice(values.as_slice()));
        }

        buffer.unmap();

        buffer
    }

    fn to_slice(&self, values: &Vec<T>) -> Option<wgpu::BufferSlice<'_>> {
        let vec_len = Self::byte_len(values);

        if vec_len == 0 {
            None

        } else {
            self.buffer.as_ref().map(|buffer| buffer.slice(..vec_len))
        }
    }

    pub(crate) fn write<'a>(&mut self, values: &Vec<T>, engine: &crate::EngineState, settings: VecBufferSettings<'a>) -> Option<wgpu::BufferSlice<'_>> {
        let vec_capacity = Self::byte_capacity(values);

        if let Some(buffer) = &self.buffer {
            let buffer_size = buffer.size();

            if buffer_size == vec_capacity {
                // TODO use StagingBelt
                engine.queue.write_buffer(buffer, 0, bytemuck::cast_slice(values.as_slice()));

            } else {
                buffer.destroy();

                if vec_capacity == 0 {
                    self.buffer = None;

                } else {
                    self.buffer = Some(Self::make_buffer(vec_capacity, values, engine, settings));
                }
            }

        } else if vec_capacity != 0 {
            self.buffer = Some(Self::make_buffer(vec_capacity, values, engine, settings));
        }

        self.to_slice(values)
    }
}

impl<T> Drop for VecBuffer<T> {
    fn drop(&mut self) {
        if let Some(buffer) = &self.buffer {
            buffer.destroy();
        }
    }
}


thread_local! {
    static INPUT_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::new());
    static OUTPUT_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::new());
    static TRANSFER_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::new());
}
*/
