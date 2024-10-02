#![no_std]

use core::{
    iter,
    ops::{Deref, DerefMut},
};

use embedded_io_async::{ErrorType, Read, ReadExactError, Write};
use futures_lite::pin;
// use futures_lite::Stream;
use mutex_trait2::{AsyncMutex, Mutex};

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub type Sid = [u8; 32];
pub trait AsyncSock: Read + Write {
    async fn send_packet(&mut self, s: Sid, x: &[u8]) -> Result<(), Self::Error>;
    async fn recv_packet(
        &mut self,
        b: &mut [u8],
    ) -> Result<(u16, Sid), ReadExactError<Self::Error>>;
}

impl<T: Read + Write> AsyncSock for T {
    async fn send_packet(&mut self, s: Sid, x: &[u8]) -> Result<(), Self::Error> {
        let mut lock = self;
        for x in x.chunks(0xffff) {
            // let mut lock = self.lock().await;
            lock.write_all(&s).await?;
            lock.write_all(&u16::to_le_bytes(x.len() as u16)).await?;
            lock.write_all(x).await?;
        }
        Ok(())
    }

    async fn recv_packet(
        &mut self,
        b: &mut [u8],
    ) -> Result<(u16, Sid), ReadExactError<Self::Error>> {
        let mut lock = self;
        // let mut lock = self.lock().await;
        let mut s = [0u8; 32];
        lock.read_exact(&mut s).await?;
        let mut l = [0u8; 2];
        lock.read_exact(&mut l).await?;
        let l = u16::from_le_bytes(l);
        lock.read_exact(&mut b[..(l as usize)]).await?;
        Ok((l, s))
    }
}
pub trait Sock: embedded_io::Read + embedded_io::Write {
    fn send_packet(&mut self, s: Sid, x: &[u8]) -> Result<(), Self::Error>;
    fn recv_packet(&mut self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>>;
}
impl<T: embedded_io::Read + embedded_io::Write> Sock for T {
    fn send_packet(&mut self, s: Sid, x: &[u8]) -> Result<(), Self::Error> {
        use embedded_io::Write;
        let mut lock = self;
        for x in x.chunks(0xffff) {
            lock.write_all(&s)?;
            lock.write_all(&u16::to_le_bytes(x.len() as u16))?;
            lock.write_all(x)?;
        }
        Ok(())
    }

    fn recv_packet(&mut self, b: &mut [u8]) -> Result<(u16, Sid), ReadExactError<Self::Error>> {
        use embedded_io::Read;
        let mut lock = self;
        let mut s = [0u8; 32];
        lock.read_exact(&mut s)?;
        let mut l = [0u8; 2];
        lock.read_exact(&mut l)?;
        let l = u16::from_le_bytes(l);
        lock.read_exact(&mut b[..(l as usize)])?;
        Ok((l, s))
    }
}

pub trait Queue {
    fn push(&self, s: Sid, a: &[u8]);
    fn pop(&mut self, s: Sid, len: usize) -> impl Iterator<Item = u8>;
}

pub trait AsyncQueue {
    async fn push(&self, s: Sid, a: &[u8]);
    async fn pop(&self, s: Sid, len: usize) -> impl futures_lite::Stream<Item = u8>;
}

#[cfg(all(feature = "alloc", feature = "once_map"))]
impl<M: Mutex<Data = alloc::collections::VecDeque<u8>> + Default> Queue
    for once_map::unsync::OnceMap<Sid, alloc::boxed::Box<M>>
{
    fn push(&self, s: Sid, a: &[u8]) {
        self.insert(s, |_| Default::default())
            .lock()
            .extend(a.iter().cloned());
    }

    fn pop(&mut self, s: Sid, len: usize) -> impl Iterator<Item = u8> {
        iter::from_fn(move || self.insert(s, |_| Default::default()).lock().pop_front())
    }
}

#[cfg(all(feature = "alloc", feature = "whisk", feature = "once_map"))]
impl AsyncQueue for once_map::unsync::OnceMap<Sid, alloc::boxed::Box<whisk::Channel<Option<u8>>>> {
    async fn push(&self, s: Sid, a: &[u8]) {
        let e = self.insert(s, |_| Default::default());
        for b in a.iter() {
            e.send(Some(*b)).await;
        }
    }

    async fn pop(&self, s: Sid, len: usize) -> impl futures_lite::Stream<Item = u8> {
        return futures_lite::stream::StreamExt::take(
            self.insert(s, |_| Default::default()).clone(),
            len,
        );
    }
}
#[derive(Clone)]
pub struct Core<S, Q> {
    pub sock: S,
    pub queue: Q,
}
impl<S: Clone, Q: Clone> Core<S, Q> {
    pub fn stream(&self, a: Sid) -> Stream<S, Q> {
        Stream {
            core: self.clone(),
            sid: a,
        }
    }
}
impl<S: AsyncSock, Q: AsyncQueue> Core<S, Q> {
    pub async fn write_async(&mut self, s: Sid, x: &[u8]) -> Result<(), S::Error> {
        self.sock.send_packet(s, x).await
    }
    pub async fn read_async(
        &mut self,
        s: Sid,
        mut a: &mut [u8],
    ) -> Result<(), ReadExactError<S::Error>> {
        let mut lock = &mut self.queue;
        {
            let st = lock.pop(s, a.len()).await;
            pin!(st);
            while let Some(b) = futures_lite::StreamExt::next(&mut st).await {
                a[0] = b;
                a = &mut a[1..];
                if a.len() == 0 {
                    return Ok(());
                }
            }
        };
        loop {
            let mut b = [0u8; 65536];
            let (a2, s2) = self.sock.recv_packet(&mut b).await?;
            if s2 != s {
                lock.push(s2, &b[..(a2 as usize)]).await;
                continue;
            }
            let c = &b[..(a2 as usize)];
            if a.len() >= a2 as usize {
                a[..(a2 as usize)].copy_from_slice(c);
                continue;
            }
            let d = &c[..a.len()];
            let e = &c[a.len()..];
            a.copy_from_slice(d);
            lock.push(s, e).await;
            return Ok(());
        }
        // loop {
        //     let mut i = 0;
        //     for (q, x) in.zip(a.iter_mut()) {
        //         *x = q;
        //         i += 1;
        //     }
        //     a = &mut a[i..];
        //     if a.len() == 0 {
        //         return Ok(());
        //     }
        //     let mut b = [0u8; 65536];
        //     let (a, s) = self.sock.recv_packet(&mut b).await?;
        //     lock.push(s, &b[..(a as usize)]);
        // }
    }
}
impl<S: Sock, Q: Queue> Core<S, Q> {
    pub fn write(&mut self, s: Sid, x: &[u8]) -> Result<(), S::Error> {
        self.sock.send_packet(s, x)
    }
    pub fn read(&mut self, s: Sid, mut a: &mut [u8]) -> Result<(), ReadExactError<S::Error>> {
        let mut lock = &mut self.queue;
        {
            let mut s = lock.pop(s, a.len());
            // pin!(s);
            while let Some(b) = s.next() {
                a[0] = b;
                a = &mut a[1..];
                if a.len() == 0 {
                    return Ok(());
                }
            }
        };
        loop {
            let mut b = [0u8; 65536];
            let (a2, s2) = self.sock.recv_packet(&mut b)?;
            if s2 != s {
                lock.push(s2, &b[..(a2 as usize)]);
                continue;
            }
            let c = &b[..(a2 as usize)];
            if a.len() >= a2 as usize {
                a[..(a2 as usize)].copy_from_slice(c);
                continue;
            }
            let d = &c[..a.len()];
            let e = &c[a.len()..];
            a.copy_from_slice(d);
            lock.push(s, e);
            return Ok(());
        }
    }
}
#[derive(Clone)]
pub struct Stream<S, Q> {
    pub core: Core<S, Q>,
    pub sid: Sid,
}
impl<S: ErrorType, Q> ErrorType for Stream<S, Q> {
    type Error = S::Error;
}
impl<S: AsyncSock, Q: AsyncQueue> Write for Stream<S, Q> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.core.write_async(self.sid, buf).await?;
        Ok(buf.len())
    }
}
impl<S: AsyncSock, Q: AsyncQueue> Read for Stream<S, Q> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        match self.core.read_async(self.sid, buf).await {
            Ok(_) => return Ok(buf.len()),
            Err(ReadExactError::Other(e)) => return Err(e),
            Err(ReadExactError::UnexpectedEof) => return Ok(0),
        }
    }
}
impl<S: Sock, Q: Queue> embedded_io::Write for Stream<S, Q> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.core.write(self.sid, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl<S: Sock, Q: Queue> embedded_io::Read for Stream<S, Q> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        match self.core.read(self.sid, buf) {
            Ok(_) => return Ok(buf.len()),
            Err(ReadExactError::Other(e)) => return Err(e),
            Err(ReadExactError::UnexpectedEof) => return Ok(0),
        }
    }
}
