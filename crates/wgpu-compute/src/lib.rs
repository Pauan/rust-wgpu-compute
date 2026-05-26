pub use wgpu_compute_macro::import_wgpu_compute;
use bytemuck::Zeroable;

mod util;
pub mod cpu;
pub mod gpu;
//mod buffer;

use cpu::IntoBuffersCpu;
use gpu::IntoBuffersGpu;


// TODO replace with Vec::from_fn after it stabilizes
pub fn empty_vec<A>(len: usize) -> Vec<A> where A: Zeroable {
    (0..len).map(|_| Zeroable::zeroed()).collect()
}


pub trait Bindings {
    type Output<'a> where Self: 'a;

    fn bindings<'a>(&'a self) -> Self::Output<'a>;
}


#[derive(Debug)]
pub enum Read<A> {
    Gpu(gpu::ReadGpu<A>),
    Cpu(cpu::ReadCpu< A>),
}

impl<A> Read<A> where A: Clone + bytemuck::AnyBitPattern {
    pub fn value(&self) -> A {
        match self {
            Self::Gpu(gpu) => gpu.value(),
            Self::Cpu(cpu) => cpu.value(),
        }
    }
}

impl<A> Read<Vec<A>> where A: Clone + bytemuck::AnyBitPattern {
    pub fn to_vec(&self) -> Vec<A> {
        match self {
            Self::Gpu(gpu) => gpu.to_vec(),
            Self::Cpu(cpu) => cpu.to_vec(),
        }
    }
}


#[derive(Debug)]
pub enum Binding<'a, GpuBuffers, A> {
    Gpu(gpu::BindingGpu<'a, GpuBuffers, A>),
    Cpu(cpu::BindingCpu<'a, A>),
}

impl<'a, GpuBuffers, A> Binding<'a, GpuBuffers, A> {
    #[inline]
    pub fn read(&self) -> Read<A> {
        match self {
            Self::Gpu(gpu) => Read::Gpu(gpu.read()),
            Self::Cpu(cpu) => Read::Cpu(cpu.read()),
        }
    }
}


pub enum State<A> where A: IntoBuffersGpu + IntoBuffersCpu {
    Gpu(gpu::StateGpu<A>),
    Cpu(cpu::StateCpu<A>),
}

impl<A> State<A> where A: IntoBuffersGpu + IntoBuffersCpu {
    pub fn new(bindings: A) -> impl Future<Output = Self> + use<A> {
        async move {
            match gpu::Gpu::get().await {
                Some(gpu) => Self::Gpu(gpu::StateGpu::new_with_gpu(IntoBuffersGpu::into_buffers_gpu(bindings, &gpu), gpu)),
                None => Self::Cpu(cpu::StateCpu::new(bindings)),
            }
        }
    }

    #[inline]
    pub fn bindings<'a>(&'a self) -> <Self as Bindings>::Output<'a> where Self: Bindings {
        <Self as Bindings>::bindings(self)
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
