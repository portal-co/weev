#![no_std]

use core::{cell::LazyCell, future::Future, ops::{Deref, DerefMut}};
use std::borrow::{Cow, ToOwned};

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

    type_param_deref_mutex!(Arc);
    type_param_deref_mutex!(Rc);
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