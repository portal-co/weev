#![no_std]

use core::{future::Future, ops::DerefMut};

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub trait Mutex {
    type Data;
    fn lock(&self) -> impl DerefMut<Target = Self::Data>;
}
#[cfg(feature = "std")]
impl<T> Mutex for std::sync::Mutex<T> {
    type Data = T;

    fn lock(&self) -> impl DerefMut<Target = Self::Data> {
        std::sync::Mutex::lock(self).expect("poison")
    }
}

pub trait AsyncMutex{
    type Data;
    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>>;
}
#[cfg(feature = "futures")]
impl<T> AsyncMutex for futures::lock::Mutex<T>{
    type Data = T;

    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
        async move{
            self.lock().await
        }
    }
}