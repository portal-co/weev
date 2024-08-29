#![no_std]

use core::ops::{Deref, DerefMut};

use embedded_io_async::{ErrorType, Read, ReadExactError, Write};
use mutex_trait2::{AsyncMutex, Mutex};

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub type Sid = [u8; 32];
pub trait AsyncSock:
    Deref<Target: AsyncMutex<Data: Read<Error = Self::Error> + Write>> + Clone + ErrorType
{
    async fn send_packet(&self, s: Sid, x: &[u8]) -> Result<(), Self::Error>;
    async fn recv_packet(&self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>>;
}
impl<T: Deref<Target: AsyncMutex<Data: Read<Error = Self::Error> + Write>> + Clone + ErrorType> AsyncSock
    for T
{
    async fn send_packet(&self, s: Sid, x: &[u8]) -> Result<(), Self::Error> {
        for x in x.chunks(0xffff) {
            let mut lock = self.lock().await;
            lock.write_all(&s).await?;
            lock.write_all(&u16::to_le_bytes(x.len() as u16)).await?;
            lock.write_all(x).await?;
        }
        Ok(())
    }

    async fn recv_packet(&self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>> {
        let mut lock = self.lock().await;
        let mut s = [0u8; 32];
        lock.read_exact(&mut s).await?;
        let mut l = [0u8; 2];
        lock.read_exact(&mut l).await?;
        let l = u16::from_le_bytes(l);
        lock.read_exact(&mut b[..(l as usize)]).await?;
        Ok((l, s))
    }
}
pub trait Sock:
    Deref<Target: Mutex<Data: embedded_io::Read<Error = Self::Error> + embedded_io::Write>> + Clone + ErrorType
{
    fn send_packet(&self, s: Sid, x: &[u8]) -> Result<(), Self::Error>;
    fn recv_packet(&self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>>;
}
impl<T: Deref<Target: Mutex<Data: embedded_io::Read<Error = Self::Error> + embedded_io::Write>> + Clone + ErrorType> Sock
    for T
{
    fn send_packet(&self, s: Sid, x: &[u8]) -> Result<(), Self::Error> {
        use embedded_io::Write;
        for x in x.chunks(0xffff) {
            let mut lock = self.lock();
            lock.write_all(&s)?;
            lock.write_all(&u16::to_le_bytes(x.len() as u16))?;
            lock.write_all(x)?;
        }
        Ok(())
    }

    fn recv_packet(&self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>> {
        use embedded_io::Read;
        let mut lock = self.lock();
        let mut s = [0u8; 32];
        lock.read_exact(&mut s)?;
        let mut l = [0u8; 2];
        lock.read_exact(&mut l)?;
        let l = u16::from_le_bytes(l);
        lock.read_exact(&mut b[..(l as usize)])?;
        Ok((l, s))
    }
}
pub trait Queue{
    fn push(&mut self, s: Sid, a: &[u8]);
    fn pop(&mut self, s: Sid, len: usize) -> impl Iterator<Item = u8>;
}
#[cfg(feature = "alloc")]
impl Queue for alloc::collections::BTreeMap<Sid,alloc::vec::Vec<u8>>{
    fn push(&mut self, s: Sid, a: &[u8]) {
        self.entry(s).or_default().extend_from_slice(a);
    }

    fn pop(&mut self, s: Sid, len: usize) -> impl Iterator<Item = u8> {
        self.entry(s).or_default().drain(..len)
    }
}
#[derive(Clone)]
pub struct Core<S,Q>{
    pub sock: S,
    pub queue: Q,
}
impl<S: Clone,Q: Clone> Core<S,Q>{
    pub fn stream(&self, a: Sid) -> Stream<S,Q>{
        Stream{
            core: self.clone(),
            sid: a,
        }
    }
}
impl<S: AsyncSock,Q: Deref<Target: AsyncMutex<Data: Queue>> + Clone> Core<S,Q>{
    pub async fn write_async(&self, s: Sid, x: &[u8]) -> Result<(),S::Error>{
        self.sock.send_packet(s, x).await
    }
    pub async fn read_async(&self, s: Sid, mut a: &mut [u8]) -> Result<(),ReadExactError<S::Error>>{
        let mut lock = self.queue.lock().await;
        loop{
            let mut i = 0;
            for (q,x) in lock.pop(s,a.len()).zip(a.iter_mut()){
                *x = q;
                i += 1;
            };
            a = &mut a[i..];
            if a.len() == 0{
                return Ok(());
            }
            let mut b = [0u8;65536];
            let (a,s) = self.sock.recv_packet(&mut b).await?;
            lock.push(s, &b[..(a as usize)]);
        }
    }
}
impl<S: Sock,Q: Deref<Target: Mutex<Data: Queue>> + Clone> Core<S,Q>{
    pub fn write(&self, s: Sid, x: &[u8]) -> Result<(),S::Error>{
        self.sock.send_packet(s, x)
    }
    pub fn read(&self, s: Sid, mut a: &mut [u8]) -> Result<(),ReadExactError<S::Error>>{
        let mut lock = self.queue.lock();
        loop{
            let mut i = 0;
            for (q,x) in lock.pop(s,a.len()).zip(a.iter_mut()){
                *x = q;
                i += 1;
            };
            a = &mut a[i..];
            if a.len() == 0{
                return Ok(());
            }
            let mut b = [0u8;65536];
            let (a,s) = self.sock.recv_packet(&mut b)?;
            lock.push(s, &b[..(a as usize)]);
        }
    }
}
#[derive(Clone)]
pub struct Stream<S,Q>{
    pub core: Core<S,Q>,
    pub sid: Sid,
}
impl<S: ErrorType,Q> ErrorType for Stream<S,Q>{
    type Error = S::Error;
}
impl<S: AsyncSock,Q: Deref<Target: AsyncMutex<Data: Queue>> + Clone> Write for Stream<S,Q>{
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.core.write_async(self.sid, buf).await?;
        Ok(buf.len())
    }
}
impl<S: AsyncSock,Q: Deref<Target: AsyncMutex<Data: Queue>> + Clone> Read for Stream<S,Q>{
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        match self.core.read_async(self.sid, buf).await{
            Ok(_) => return Ok(buf.len()),
            Err(ReadExactError::Other(e)) => return Err(e),
            Err(ReadExactError::UnexpectedEof) => return Ok(0)
        }
    }
}
impl<S: Sock,Q: Deref<Target: Mutex<Data: Queue>> + Clone> embedded_io::Write for Stream<S,Q>{
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.core.write(self.sid, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl<S: Sock,Q: Deref<Target: Mutex<Data: Queue>> + Clone> embedded_io::Read for Stream<S,Q>{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        match self.core.read(self.sid, buf){
            Ok(_) => return Ok(buf.len()),
            Err(ReadExactError::Other(e)) => return Err(e),
            Err(ReadExactError::UnexpectedEof) => return Ok(0)
        }
    }
}