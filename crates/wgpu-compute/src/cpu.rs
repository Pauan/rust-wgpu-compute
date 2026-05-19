pub struct ReadCpu<'a, A> {
    mutex: &'a Mutex<A>,
}

impl<'a, A> ReadCpu<'a, A> where A: Clone {
    #[inline]
    pub fn value(&self) -> A {
        self.mutex.lock().unwrap().clone()
    }
}

impl<'a, A> ReadCpu<'a, Vec<A>> where A: Clone {
    #[inline]
    pub fn to_vec(&self) -> Vec<A> {
        self.mutex.lock().unwrap().clone()
    }
}


pub struct BindingCpu<'a, A> {
    mutex: &'a Mutex<A>,
}

impl<'a, A> BindingCpu<'a, A> {
    #[inline]
    pub fn read(&self) -> ReadCpu<'a, A> {
        ReadCpu {
            mutex: &self.mutex,
        }
    }
}


#[non_exhaustive]
pub struct StateCpu<A> {
    #[doc(hidden)]
    pub buffers: A,
}

impl<A> StateCpu<A> {
    #[inline]
    pub fn new<B>(bindings: B) -> Self where B: IntoBuffersCpu {
        Self {
            buffers: bindings.into_buffers(),
        }
    }

    #[inline]
    pub fn bindings<B>(&self) -> B::Output where B: Bindings<Self> {
        B::bindings(self)
    }
}
