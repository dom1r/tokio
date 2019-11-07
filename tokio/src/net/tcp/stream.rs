use crate::io::{AsyncRead, AsyncWrite};
use crate::net::tcp::split::{split, ReadHalf, WriteHalf};
use crate::net::util::IoResource;
use crate::net::ToSocketAddrs;

use bytes::{Buf, BufMut};
use futures_core::ready;
use futures_util::future::poll_fn;
use std::convert::TryFrom;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{self, Shutdown, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};

/// An I/O object representing a TCP stream connected to a remote endpoint.
///
/// A TCP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
///
/// [`connect`]: struct.TcpStream.html#method.connect
/// [accepting]: struct.TcpListener.html#method.accept
/// [listener]: struct.TcpListener.html
///
/// # Examples
///
/// ```no_run
/// use tokio::net::TcpStream;
/// use tokio::prelude::*;
/// use std::error::Error;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn Error>> {
///     // Connect to a peer
///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
///
///     // Write some data.
///     stream.write_all(b"hello world!").await?;
///
///     Ok(())
/// }
/// ```
pub struct TcpStream {
    io: IoResource<mio::net::TcpStream>,
}

impl TcpStream {
    /// Opens a TCP connection to a remote host.
    ///
    /// `addr` is an address of the remote host. Anything which implements
    /// `ToSocketAddrs` trait can be supplied for the address.
    ///
    /// If `addr` yields multiple addresses, connect will be attempted with each
    /// of the addresses until a connection is successful. If none of the
    /// addresses result in a successful connection, the error returned from the
    /// last connection attempt (the last address) is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     // Write some data.
    ///     stream.write_all(b"hello world!").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addrs = addr.to_socket_addrs().await?;

        let mut last_err = None;

        for addr in addrs {
            match TcpStream::connect_addr(addr).await {
                Ok(stream) => return Ok(stream),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any addresses",
            )
        }))
    }

    /// Establish a connection to the specified `addr`.
    async fn connect_addr(addr: SocketAddr) -> io::Result<TcpStream> {
        let sys = mio::net::TcpStream::connect(addr)?;
        let stream = TcpStream::new(sys)?;

        // Once we've connected, wait for the stream to be writable as
        // that's when the actual connection has been initiated. Once we're
        // writable we check for `take_socket_error` to see if the connect
        // actually hit an error or not.
        //
        // If all that succeeded then we ship everything on up.
        poll_fn(|cx| stream.io.poll_write_ready(cx)).await?;

        if let Some(e) = stream.io.get_ref().take_error()? {
            return Err(e);
        }

        Ok(stream)
    }

    pub(crate) fn new(connected: mio::net::TcpStream) -> io::Result<TcpStream> {
        let io = IoResource::new(connected)?;
        Ok(TcpStream { io })
    }

    /// Create a new `TcpStream` from a `std::net::TcpStream`.
    ///
    /// This function will convert a TCP stream created by the standard library
    /// to a TCP stream ready to be used with the provided event loop handle.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::error::Error;
    /// use tokio::net::TcpStream;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let std_stream = std::net::TcpStream::connect("127.0.0.1:34254")?;
    ///     let stream = TcpStream::from_std(std_stream)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn from_std(stream: net::TcpStream) -> io::Result<TcpStream> {
        let io = mio::net::TcpStream::from_std(stream);
        let io = IoResource::new(io)?;
        Ok(TcpStream { io })
    }

    // Connect a TcpStream asynchronously that may be built with a net2 TcpBuilder.
    //
    // This should be removed in favor of some in-crate TcpSocket builder API.
    #[doc(hidden)]
    pub async fn connect_std(_stream: net::TcpStream, _addr: &SocketAddr) -> io::Result<TcpStream> {
        // let io = mio::net::TcpStream::connect_stream(stream, addr)?;
        // let io = IoResource::new(io)?;
        // let stream = TcpStream { io };

        // Once we've connected, wait for the stream to be writable as
        // that's when the actual connection has been initiated. Once we're
        // writable we check for `take_socket_error` to see if the connect
        // actually hit an error or not.
        //
        // If all that succeeded then we ship everything on up.
        // poll_fn(|cx| stream.io.poll_write_ready(cx)).await?;

        // if let Some(e) = stream.io.get_ref().take_error()? {
        //     return Err(e);
        // }

        // Ok(stream)
        unimplemented!()
    }

    /// Returns the local address that this stream is bound to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.local_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the remote address that this stream is connected to.
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.peer_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
    }

    fn poll_peek(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx))?;

        match self.io.get_ref().peek(buf) {
            Ok(ret) => Poll::Ready(Ok(ret)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx)?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    /// Receives data on the socket from the remote address to which it is
    /// connected, without removing that data from the queue. On success,
    /// returns the number of bytes peeked.
    ///
    /// Successive calls return the same data. This is accomplished by passing
    /// `MSG_PEEK` as a flag to the underlying recv system call.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     let mut b1 = [0; 10];
    ///     let mut b2 = [0; 10];
    ///
    ///     // Peek at the data
    ///     let n = stream.peek(&mut b1).await?;
    ///
    ///     // Read the data
    ///     assert_eq!(n, stream.read(&mut b2[..n]).await?);
    ///     assert_eq!(&b1[..n], &b2[..n]);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_peek(cx, buf)).await
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified
    /// portions to return immediately with an appropriate value (see the
    /// documentation of `Shutdown`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::net::Shutdown;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     // Shutdown the stream
    ///     stream.shutdown(Shutdown::Write)?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`].
    ///
    /// [`set_nodelay`]: #method.set_nodelay
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.nodelay()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn nodelay(&self) -> io::Result<bool> {
        self.io.get_ref().nodelay()
    }

    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_nodelay(true)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.io.get_ref().set_nodelay(nodelay)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: #tymethod.set_ttl
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.ttl()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn ttl(&self) -> io::Result<u32> {
        self.io.get_ref().ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_ttl(123)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_ttl(ttl)
    }

    /// Split a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// See the module level documenation of [`split`](super::split) for more
    /// details.
    pub fn split(&mut self) -> (ReadHalf<'_>, WriteHalf<'_>) {
        split(self)
    }

    // == Poll IO functions that takes `&self` ==
    //
    // They are not public because (taken from the doc of `PollEvented`):
    //
    // While `PollEvented` is `Sync` (if the underlying I/O type is `Sync`), the
    // caller must ensure that there are at most two tasks that use a
    // `PollEvented` instance concurrently. One for reading and one for writing.
    // While violating this requirement is "safe" from a Rust memory model point
    // of view, it will result in unexpected behavior in the form of lost
    // notifications and tasks hanging.

    pub(crate) fn poll_read_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx))?;

        match self.io.get_ref().read(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    pub(crate) fn poll_read_buf_priv<B: BufMut>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx))?;

        let r = unsafe { self.io.get_ref().read(buf.bytes_mut()) };
        match r {
            Ok(n) => {
                unsafe {
                    buf.advance_mut(n);
                }
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx)?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    pub(crate) fn poll_write_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    pub(crate) fn poll_write_buf_priv<B: Buf>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        let r = self.io.get_ref().write(buf.bytes());
        match r {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl TryFrom<TcpStream> for mio::net::TcpStream {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::util::PollEvented::into_inner
    fn try_from(value: TcpStream) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::TcpStream> for TcpStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`TcpStream::from_std(stream)`](TcpStream::from_std).
    fn try_from(stream: net::TcpStream) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

// ===== impl Read / Write =====

impl AsyncRead for TcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_priv(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_read_buf_priv(cx, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_priv(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // tcp flush is a no-op
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.shutdown(std::net::Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_write_buf_priv(cx, buf)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

#[cfg(unix)]
mod sys {
    use super::TcpStream;
    use std::os::unix::prelude::*;

    impl AsRawFd for TcpStream {
        fn as_raw_fd(&self) -> RawFd {
            self.io.get_ref().as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    // TODO: let's land these upstream with mio and then we can add them here.
    //
    // use std::os::windows::prelude::*;
    // use super::TcpStream;
    //
    // impl AsRawHandle for TcpStream {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.io.get_ref().as_raw_handle()
    //     }
    // }
}
