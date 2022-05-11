use std::{future::Future, pin::Pin, task::Poll};

#[pin_project::pin_project]
pub struct Read<'a, T: Unpin> {
    buf: super::ReadBuf<'a>,
    #[pin]
    reader: &'a mut T,
}

#[pin_project::pin_project]
pub struct Write<'a, T: Unpin> {
    buf: &'a [u8],
    #[pin]
    writer: &'a mut T,
}

#[pin_project::pin_project]
pub struct ReadExact<'a, T> {
    buf: super::ReadBuf<'a>,
    #[pin]
    reader: &'a mut T,
}

#[pin_project::pin_project]
pub struct WriteAll<'a, T> {
    buf: &'a [u8],
    offset: usize,
    #[pin]
    writer: &'a mut T,
}

pub trait AsyncReadExt: super::AsyncRead {

    #[inline]
    fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> Read<'a, Self>
    where
        Self: Sized + Unpin,
    {
        Read {
            #[cfg(feature = "fuso-rt-tokio")]
            buf: super::ReadBuf::new(tokio::io::ReadBuf::new(buf)),
            #[cfg(any(feature = "fuso-rt-smol", feature = "fuso-rt-custom"))]
            buf: super::ReadBuf::new(buf),
            reader: self,
        }
    }

    #[inline]
    fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self>
    where
        Self: Sized + Unpin,
    {
        ReadExact {
            #[cfg(feature = "fuso-rt-tokio")]
            buf: super::ReadBuf::new(tokio::io::ReadBuf::new(buf)),
            #[cfg(any(feature = "fuso-rt-smol", feature = "fuso-rt-custom"))]
            buf: super::ReadBuf::new(buf),
            reader: self,
        }
    }
}

pub trait AsyncWriteExt: super::AsyncWrite {
    #[inline]
    fn write<'a>(&'a mut self, buf: &'a [u8]) -> Write<'a, Self>
    where
        Self: Sized + Unpin,
    {
        Write {
            buf: buf,
            writer: self,
        }
    }

    #[inline]
    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Sized + Unpin,
    {
        WriteAll {
            buf,
            offset: 0,
            writer: self,
        }
    }
}

impl<T> AsyncReadExt for T where T: super::AsyncRead + Unpin {}
impl<T> AsyncWriteExt for T where T: super::AsyncWrite + Unpin {}

impl<'a, T> Future for Read<'a, T>
where
    T: super::AsyncRead + Unpin,
{
    type Output = crate::Result<usize>;

    #[inline]
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        Pin::new(&mut **this.reader).poll_read(cx, this.buf)
    }
}

impl<'a, T> Future for Write<'a, T>
where
    T: super::AsyncWrite + Unpin,
{
    type Output = crate::Result<usize>;

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        Pin::new(&mut **this.writer).poll_write(cx, *this.buf)
    }
}

#[inline]
fn eof() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "early eof")
}

impl<'a, T> Future for ReadExact<'a, T>
where
    T: super::AsyncRead + Unpin,
{
    type Output = crate::Result<()>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();

        let buf = this.buf;
        let mut reader = this.reader;

        loop {
            let rem = buf.remaining();
            if rem != 0 {
                match Pin::new(&mut **reader).poll_read(cx, buf)? {
                    Poll::Pending => break Poll::Pending,
                    Poll::Ready(_) => {
                        if rem == buf.remaining() {
                            break Poll::Ready(Err(eof().into()));
                        }
                    }
                }
            } else {
                break Poll::Ready(Ok(()));
            }
        }
    }
}

impl<'a, T> Future for WriteAll<'a, T>
where
    T: super::AsyncWrite + Unpin,
{
    type Output = crate::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut writer = this.writer;
        let offset = this.offset;

        loop {
            match Pin::new(&mut **writer).poll_write(cx, &this.buf[*offset..])? {
                Poll::Pending => break Poll::Pending,
                Poll::Ready(n) => {
                    *offset += n;
                }
            }

            if *offset == this.buf.len() {
                break Poll::Ready(Ok(()));
            }
        }
    }
}
