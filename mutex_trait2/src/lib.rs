#![no_std]

use core::{cell::LazyCell, future::Future, ops::{Deref, DerefMut}};
// use std::borrow::{Cow, ToOwned};

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
impl<'a,T: Mutex> Mutex for &'a T{
    type Data = T::Data;

    fn lock(&self) -> impl DerefMut<Target = Self::Data> {
        (**self).lock()
    }
}
impl<'a,T: AsyncMutex> AsyncMutex for &'a T{
    type Data = T::Data;

    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
        (**self).lock()
    }
}

impl<T: Mutex,F: FnOnce() -> T> Mutex for LazyCell<T,F>{
    type Data = T::Data;

    fn lock(&self) -> impl DerefMut<Target = Self::Data> {
        (**self).lock()
    }
}
impl<T: AsyncMutex,F: FnOnce() -> T> AsyncMutex for LazyCell<T,F>{
    type Data = T::Data;

    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
        (**self).lock()
    }
}
#[macro_export]
macro_rules! type_param_deref_mutex {
    ($a:ident) => {
        impl<T: $crate::Mutex> $crate::Mutex for $a<T>{
            type Data = T::Data;
        
            fn lock(&self) -> impl DerefMut<Target = Self::Data> {
                (**self).lock()
            }
        }
        impl<T: $crate::AsyncMutex> $crate::AsyncMutex for $a<T>{
            type Data = T::Data;
        
            fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
                (**self).lock()
            }
        }
    };
}
#[cfg(feature = "alloc")]
const _: () = {
    use alloc::rc::Rc;

    use alloc::sync::Arc;
    use alloc::borrow::*;

    type_param_deref_mutex!(Arc);
    type_param_deref_mutex!(Rc);
    impl<'a,T: Mutex + ToOwned> Mutex for Cow<'a,T>{
        type Data = T::Data;
    
        fn lock(&self) -> impl DerefMut<Target = Self::Data> {
            (**self).lock()
        }
    }
    impl<'a,T: AsyncMutex + ToOwned> AsyncMutex for Cow<'a,T>{
        type Data = T::Data;
    
        fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
            (**self).lock()
        }
    }
};
#[cfg(feature = "std")]
const _: () = {
    use std::sync::LazyLock;

    impl<T: Mutex,F: FnOnce() -> T> Mutex for LazyLock<T,F>{
        type Data = T::Data;
    
        fn lock(&self) -> impl DerefMut<Target = Self::Data> {
            (**self).lock()
        }
    }
    impl<T: AsyncMutex,F: FnOnce() -> T> AsyncMutex for LazyLock<T,F>{
        type Data = T::Data;
    
        fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
            (**self).lock()
        }
    }
};
#[cfg(feature = "futures")]
impl<T> AsyncMutex for futures::lock::Mutex<T>{
    type Data = T;

    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
        async move{
            self.lock().await
        }
    }
}
#[cfg(feature = "embassy-sync")]
const _: () = {
    use embassy_sync::blocking_mutex::raw::RawMutex;

    impl<M: RawMutex,T> AsyncMutex for embassy_sync::mutex::Mutex<M,T>{
        type Data = T;
    
        fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
            self.lock()
        }
    }
};
#[derive(Clone)]
pub struct PreShared<T: Clone>{
    pub contents: T,
}
impl<T: Clone> Deref for PreShared<T>{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}
impl<T: Clone> DerefMut for PreShared<T>{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.contents
    }
}
impl<T: Clone> Mutex for PreShared<T>{
    type Data = T;

    fn lock(&self) -> impl DerefMut<Target = Self::Data> {
       self.clone()
    }
}
impl<T: Clone> AsyncMutex for PreShared<T>{
    type Data = T;

    fn lock(&self) -> impl Future<Output: DerefMut<Target = Self::Data>> {
        core::future::ready(self.clone())
    }
}
#[derive(Clone)]
pub struct M<T>(pub T);
#[derive(Clone)]
pub struct AM<T>(pub T);
#[cfg(feature = "embedded-io")]
const _: () = {
    impl<T: Mutex<Data: embedded_io::ErrorType>> embedded_io::ErrorType for M<T>{
        type Error = <T::Data as embedded_io::ErrorType>::Error;
    }
    impl<T: Mutex<Data: embedded_io::Read>> embedded_io::Read for M<T>{
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            self.0.lock().read(buf)
        }
    }
    impl<T: Mutex<Data: embedded_io::Write>> embedded_io::Write for M<T>{
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.0.lock().write(buf)
        }
    
        fn flush(&mut self) -> Result<(), Self::Error> {
            self.0.lock().flush()
        }
    }
};
#[cfg(feature = "embedded-io-async")]
const _: () = {
    impl<T: AsyncMutex<Data: embedded_io_async::ErrorType>> embedded_io_async::ErrorType for AM<T>{
        type Error = <T::Data as embedded_io_async::ErrorType>::Error;
    }
    impl<T: AsyncMutex<Data: embedded_io_async::Read>> embedded_io_async::Read for AM<T>{
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            self.0.lock().await.read(buf).await
        }
    }
    impl<T: AsyncMutex<Data: embedded_io_async::Write>> embedded_io_async::Write for AM<T>{
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.0.lock().await.write(buf).await
        }
    
        async fn flush(&mut self) -> Result<(), Self::Error> {
            self.0.lock().await.flush().await
        }
    }
};