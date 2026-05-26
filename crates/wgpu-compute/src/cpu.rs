use std::sync::{Arc, Mutex};
use crate::Bindings;


pub trait IntoBuffersCpu {
    type Buffers;

    fn into_buffers_cpu(self) -> Self::Buffers;
}


#[derive(Debug)]
pub struct ReadCpu<A> {
    mutex: Arc<Mutex<A>>,
}

impl<A> ReadCpu<A> where A: Clone {
    #[inline]
    pub fn value(&self) -> A {
        self.mutex.lock().unwrap().clone()
    }
}

impl<A> ReadCpu<Vec<A>> where A: Clone {
    #[inline]
    pub fn to_vec(&self) -> Vec<A> {
        self.mutex.lock().unwrap().clone()
    }
}


#[derive(Debug)]
pub struct BindingCpu<'a, A> {
    mutex: &'a Arc<Mutex<A>>,
}

impl<'a, A> BindingCpu<'a, A> {
    #[doc(hidden)]
    #[inline]
    pub fn new<B>(mutex: &'a Arc<Mutex<A>>) -> Self {
        Self { mutex }
    }

    #[inline]
    pub fn read(&self) -> ReadCpu<A> {
        ReadCpu {
            mutex: self.mutex.clone(),
        }
    }
}


#[derive(Debug)]
#[non_exhaustive]
pub struct StateCpu<A> where A: IntoBuffersCpu {
    #[doc(hidden)]
    pub buffers: A::Buffers,
}

impl<A> StateCpu<A> where A: IntoBuffersCpu {
    #[inline]
    pub fn new(bindings: A) -> Self {
        Self {
            buffers: bindings.into_buffers_cpu(),
        }
    }

    #[inline]
    pub fn bindings<'a>(&'a self) -> <Self as Bindings>::Output<'a> where Self: Bindings {
        <Self as Bindings>::bindings(self)
    }
}
