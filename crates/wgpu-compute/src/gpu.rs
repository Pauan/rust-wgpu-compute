use std::marker::PhantomData;
use std::cell::RefCell;
use bytemuck::NoUninit;
use crate::Bindings;


pub trait IntoBuffersGpu {
    type Buffers;

    fn into_buffers_gpu(self, gpu: &Gpu) -> Self::Buffers;
}


#[derive(Debug, Clone)]
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


    pub(crate) fn get() -> impl Future<Output = Option<Self>> + use<> {
        thread_local! {
            static GPU: RefCell<crate::util::Loading<Option<Gpu>>> = RefCell::new(crate::util::Loading::Init);
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


    #[doc(hidden)]
    pub fn input<A>(&self, value: &A) -> BufferGpu<A> where A: NoUninit {
        BufferGpu {
            buffer: self.input_buffer(bytemuck::bytes_of(value)),
            _phantom: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn input_vec<A>(&self, value: &[A]) -> BufferGpu<Vec<A>> where A: NoUninit {
        BufferGpu {
            buffer: self.input_buffer(bytemuck::cast_slice(value)),
            _phantom: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn bind_group<'a, A>(input: &'a BufferGpu<A>) -> wgpu::BindingResource<'a> {
        input.buffer.as_entire_binding()
    }
}


#[doc(hidden)]
#[derive(Debug)]
pub struct BufferGpu<A> {
    buffer: wgpu::Buffer,
    _phantom: PhantomData<A>,
}


#[derive(Debug)]
pub struct BindingGpu<'a, Buffers, A> {
    state: &'a __internal::StateGpu<Buffers>,
    buffer: &'a BufferGpu<A>,
}

impl<'a, Buffers, A> BindingGpu<'a, Buffers, A> {
    #[doc(hidden)]
    #[inline]
    pub fn new(state: &'a __internal::StateGpu<Buffers>, buffer: &'a BufferGpu<A>) -> Self {
        Self { state, buffer }
    }

    #[inline]
    pub fn read(&self) -> ReadGpu<A> {
        self.state.copy_buffer(&self.buffer.buffer)
    }
}


#[derive(Debug)]
pub struct ReadGpu<A> {
    buffer: wgpu::Buffer,
    _phantom: PhantomData<A>,
}

impl<A> ReadGpu<A> where A: bytemuck::AnyBitPattern {
    pub fn value(&self) -> A {
        let data = self.buffer.get_mapped_range(..);
        *bytemuck::from_bytes(&data)
    }
}

impl<A> ReadGpu<Vec<A>> where A: bytemuck::AnyBitPattern {
    pub fn to_vec(&self) -> Vec<A> {
        let data = self.buffer.get_mapped_range(..);
        bytemuck::cast_slice(&data).to_vec()
    }
}


#[derive(Debug)]
#[non_exhaustive]
pub struct StateGpu<A>(__internal::StateGpu<A::Buffers>) where A: IntoBuffersGpu;

impl<A> StateGpu<A> where A: IntoBuffersGpu {
    pub(crate) fn new_with_gpu(buffers: A::Buffers, gpu: Gpu) -> Self {
        Self(__internal::StateGpu {
            buffers,
            gpu,
            copy_buffers: RefCell::new(vec![]),
        })
    }

    pub fn new(bindings: A) -> impl Future<Output = Option<Self>> + use<A> {
        async move {
            match Gpu::get().await {
                Some(gpu) => Some(Self::new_with_gpu(bindings.into_buffers_gpu(&gpu), gpu)),
                None => None,
            }
        }
    }

    #[inline]
    pub fn bindings<'a>(&'a self) -> <Self as Bindings>::Output<'a> where Self: Bindings {
        <Self as Bindings>::bindings(self)
    }

    #[doc(hidden)]
    #[inline]
    pub fn __internal(&self) -> &__internal::StateGpu<A::Buffers> {
        &self.0
    }
}


#[doc(hidden)]
pub mod __internal {
    use super::{Gpu, ReadGpu};
    use std::cell::RefCell;
    use std::marker::PhantomData;
    use crate::util::make_empty_callback;


    #[derive(Debug)]
    pub struct StateGpu<A> {
        pub buffers: A,

        pub gpu: Gpu,

        pub copy_buffers: RefCell<Vec<(wgpu::Buffer, wgpu::Buffer)>>,
    }

    impl<A> StateGpu<A> {
        pub fn copy_buffer<B>(&self, input_buffer: &wgpu::Buffer) -> ReadGpu<B> {
            let output_buffer = self.gpu.output_buffer(input_buffer);

            self.copy_buffers.borrow_mut().push((input_buffer.clone(), output_buffer.clone()));

            ReadGpu {
                buffer: output_buffer,
                _phantom: PhantomData,
            }
        }
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
