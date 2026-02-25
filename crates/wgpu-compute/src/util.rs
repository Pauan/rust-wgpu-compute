use futures::channel::oneshot;


/*fn make_callback<A>() -> (impl FnOnce(A), impl Future<Output = A>) {
    let (sender, receiver) = oneshot::channel();

    let callback = move |result| {
        let _ = sender.send(result);
    };

    (callback, async move { receiver.await.unwrap() })
}*/


pub fn make_empty_callback() -> (impl FnOnce(), impl Future<Output = ()>) {
    let (sender, receiver) = oneshot::channel();

    let callback = move || {
        let _ = sender.send(());
    };

    (callback, async move { receiver.await.unwrap() })
}


pub enum Loading<A> {
    Init,
    Pending(Vec<oneshot::Sender<()>>),
    Done(A),
}

impl<A> Loading<A> {
    pub fn init<B, F>(&mut self, f: F) -> Option<B> where F: FnOnce() -> B {
        if let Self::Init = *self {
            *self = Self::Pending(vec![]);
            Some(f())

        } else {
            None
        }
    }

    pub fn wait(&mut self) -> Option<oneshot::Receiver<()>> {
        match self {
            Self::Init => {
                unreachable!();
            },
            Self::Pending(senders) => {
                let (sender, receiver) = oneshot::channel();

                senders.push(sender);

                Some(receiver)
            },
            Self::Done(_) => {
                None
            },
        }
    }

    pub fn done(&mut self, value: A) {
        let mut old = Self::Done(value);

        std::mem::swap(self, &mut old);

        match old {
            Self::Init | Self::Done(_) => unreachable!(),
            Self::Pending(senders) => {
                for sender in senders {
                    let _ = sender.send(());
                }
            },
        }
    }
}

impl<A> Loading<A> where A: Clone {
    pub fn value(&self) -> A {
        match self {
            Self::Init | Self::Pending(_) => unreachable!(),
            Self::Done(value) => value.clone(),
        }
    }
}


/*
fn buffer_size(old_capacity: u64, min_size: u64) -> u64 {
    let mut new_capacity = if old_capacity == 0 {
        wgpu::COPY_BUFFER_ALIGNMENT

    } else {
        old_capacity
    };

    // Exponential size increase, similar to Vec
    while new_capacity < min_size {
        new_capacity = new_capacity * 2;
    }

    assert_eq!(new_capacity % wgpu::COPY_BUFFER_ALIGNMENT, 0);

    new_capacity
}


#[derive(Clone, Copy, PartialEq)]
pub struct Allocation {
    start: u64,
    end: u64,
}

/// Allocates a single large buffer and creates sub-slices as needed
pub struct BufferPool {
    usages: wgpu::BufferUsages,
    capacity: u64,
    buffer: Option<Buffer>,
    allocated: Vec<Allocation>,
}

impl BufferPool {
    pub fn new(usages: wgpu::BufferUsages) -> Self {
        Self {
            usages,
            capacity: 0,
            buffer: None,
            allocated: vec![],
        }
    }

    fn resize_to(&mut self, min_size: u64) {
        if self.capacity < min_size {
            self.capacity = buffer_size(self.capacity, min_size);
        }
    }

    fn find_allocation(&self, size: u64) -> (Result<usize, u64>, Allocation) {
        assert!(size > 0);

        let mut start = 0;

        for (index, allocation) in self.allocated.iter().enumerate() {
            let end = start + size;

            // Found slice that fits, just return it
            if end <= allocation.start {
                return (Ok(index), Allocation { start, end });

            } else {
                start = allocation.end;
            }
        }

        // We didn't find any sub-slices, so try to create slice at end
        let end = start + size;

        let allocation = Allocation { start, end };

        // We have enough space at the end
        if end <= self.capacity {
            (Ok(self.allocated.len()), allocation)

        // We don't have enough space, need to resize buffer
        } else {
            (Err(end), allocation)
        }
    }

    fn reallocate(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if let Some(old_buffer) = self.buffer {
            // Need to reallocate buffer
            if old_buffer.size() < self.capacity {
                let new_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: self.capacity,
                    usage: self.usages,
                    mapped_at_creation: false,
                });

                encoder.copy_buffer_to_buffer(
                    old_buffer,
                    0,
                    &new_buffer,
                    0,
                    None,
                );

                self.buffer = Some(new_buffer);
            }

        } else {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: self.capacity,
                usage: self.usages,
                mapped_at_creation: false,
            }));
        }
    }

    pub fn allocate(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, size: u64) -> Allocation {
        match self.find_allocation(size) {
            (Ok(index), allocation) => {
                self.allocated.insert(index, allocation);
                allocation
            },
            (Err(min_size), allocation) => {
                self.resize_to(min_size);
                self.allocated.push(allocation);
                self.reallocate(device, encoder);
                allocation
            },
        }
    }

    pub fn deallocate(&mut self, allocation: Allocation) {
        if let Some(index) = self.allocated.iter().position(|x| x == allocation) {
            self.allocated.remove(index);
        }
    }

    pub fn read_write(&self) -> impl Future<Output = ReadWrite> + use<> {
        let buffer = self.buffer.clone();

        let (callback, future) = make_callback();

        self.buffer.map_async(wgpu::MapMode::Write, callback);

        async move {
            future.await.unwrap();

            ReadWrite {}
        }
    }

    pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, data: &[u8]) -> Allocation {
        let allocation = self.allocate(device, encoder, data.len());

        queue.write_buffer(self.get_slice(&allocation);

        assert_eq!(data.len() as u64);


        allocation
    }

    pub fn get_slice(&self, allocation: &Allocation) -> wgpu::BufferSlice {
        self.buffer.unwrap().slice(allocation.start..allocation.end)
    }
}
*/
