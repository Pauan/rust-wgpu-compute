pub struct BufferVec<A> {
    input_buffer: Option<wgpu::Buffer>,
    output_buffer: Option<wgpu::Buffer>,
    is_empty: bool,
    is_output: bool,
    value: Vec<A>,
}

impl<A> BufferVec<A> {
    pub fn new(value: Vec<A>) -> Self {
        Self {
            input_buffer: None,
            output_buffer: None,
            is_empty: false,
            is_output: false,
            value,
        }
    }

    pub fn len(&self) -> usize {
        self.value.len()
    }

    pub fn output(mut self, value: bool) -> Self {
        self.is_output = value;
        self
    }

    pub(crate) fn input_buffer(&self, gpu: &Gpu) -> &wgpu::Buffer {
        self.input_buffer.get_or_insert_with(|| {
            let value = bytemuck::cast_slice(&self.value);

            // Buffer which contains the input data, which will be sent to the GPU.
            let input_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: value.len() as u64,
                usage: if self.is_output {
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC
                } else {
                    wgpu::BufferUsages::STORAGE
                },
                mapped_at_creation: true,
            });

            input_buffer.slice(..)
                .get_mapped_range_mut()[..]
                .copy_from_slice(value);

            input_buffer.unmap();

            input_buffer
        })



        size_of_val(&*self.value)
    }

    pub(crate) fn output_buffer(&self, gpu: &Gpu) -> Option<wgpu::Buffer> {
        // Buffer which the GPU will write into.
            let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });

            // We have to use a separate buffer for transferring from the GPU to the CPU,
            // because the browser does not allow for combining `MAP_READ` with `STORAGE`.
            let transfer_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            (output_buffer, transfer_buffer)
    }
}

impl<A> BufferVec<A> where A: Default {
    pub fn empty(size: usize) -> Self {
        let mut this = Self::new(vec![Default::default(); size]);
        this.is_empty = true;
        this
    }
}


pub struct OutputVec<A> {
    buffer: Option<wgpu::Buffer>,
    value: Option<Vec<A>>,
}

impl<A> OutputVec<A> {

}
