mod utils;
#[cfg(windows)]
mod windows;

use cfg_if::cfg_if;
use std::io;
use std::mem;
use std::ops::Neg;
use std::time::Duration;

#[cfg(any(unix, target_os = "redox"))]
use libc::*;
#[cfg(any(unix, target_os = "redox"))]
use std::os::unix::prelude::*;
#[cfg(windows)]
use std::os::windows::prelude::*;

#[cfg(windows)]
use windows::*;

#[cfg(target_os = "redox")]
type Socket = usize;
#[cfg(unix)]
type Socket = c_int;
#[cfg(windows)]
type Socket = SOCKET;

#[cfg(windows)]
const SIO_KEEPALIVE_VALS: DWORD = 0x98000004;
#[cfg(windows)]
#[repr(C)]
struct tcp_keepalive {
    onoff: c_ulong,
    keepalivetime: c_ulong,
    keepaliveinterval: c_ulong,
}

#[cfg(any(unix, target_os = "redox"))]
fn v(opt: c_int) -> c_int {
    opt
}
#[cfg(windows)]
fn v(opt: IPPROTO) -> c_int {
    opt as c_int
}

fn set_opt<T: Copy>(sock: Socket, opt: c_int, val: c_int, payload: T) -> io::Result<()> {
    unsafe {
        let payload = &payload as *const T as *const c_void;
        #[cfg(target_os = "redox")]
        let sock = sock as c_int;
        cvt(setsockopt(
            sock,
            opt,
            val,
            payload as *const _,
            mem::size_of::<T>() as socklen_t,
        ))?;
    }
    Ok(())
}

fn get_opt<T: Copy>(sock: Socket, opt: c_int, val: c_int) -> io::Result<T> {
    unsafe {
        let mut slot: T = mem::zeroed();
        let mut len = mem::size_of::<T>() as socklen_t;
        #[cfg(target_os = "redox")]
        let sock = sock as c_int;
        cvt(getsockopt(
            sock,
            opt,
            val,
            &mut slot as *mut _ as *mut _,
            &mut len,
        ))?;
        assert_eq!(len as usize, mem::size_of::<T>());
        Ok(slot)
    }
}

pub(crate) trait AsSock {
    fn as_sock(&self) -> Socket;
}

#[cfg(any(unix, target_os = "redox"))]
impl<T: AsRawFd> AsSock for T {
    fn as_sock(&self) -> Socket {
        self.as_raw_fd()
    }
}
#[cfg(windows)]
impl<T: AsRawSocket> AsSock for T {
    fn as_sock(&self) -> Socket {
        self.as_raw_socket() as Socket
    }
}

cfg_if! {
    if #[cfg(any(target_os = "macos", target_os = "ios"))] {
        use libc::TCP_KEEPALIVE as KEEPALIVE_OPTION;
    } else if #[cfg(any(target_os = "openbsd", target_os = "netbsd"))] {
        use libc::SO_KEEPALIVE as KEEPALIVE_OPTION;
    } else if #[cfg(any(unix, target_os = "redox"))] {
        use libc::TCP_KEEPIDLE as KEEPALIVE_OPTION;
    } else {
        // ...
    }
}

pub(crate) trait SockOpt {
    fn set_recv_buffer_size(&self, size: usize) -> io::Result<()>;

    fn recv_buffer_size(&self) -> io::Result<usize>;

    fn set_send_buffer_size(&self, size: usize) -> io::Result<()>;

    fn send_buffer_size(&self) -> io::Result<usize>;

    fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()>;

    fn keepalive(&self) -> io::Result<Option<Duration>>;

    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()>;

    fn keepalive_ms(&self) -> io::Result<Option<u32>>;

    fn set_linger(&self, dur: Option<Duration>) -> io::Result<()>;

    fn linger(&self) -> io::Result<Option<Duration>>;
}

impl<T: AsSock> SockOpt for T {
    fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        // TODO: casting usize to a c_int should be a checked cast
        set_opt(self.as_sock(), SOL_SOCKET, SO_RCVBUF, size as c_int)
    }

    fn recv_buffer_size(&self) -> io::Result<usize> {
        get_opt(self.as_sock(), SOL_SOCKET, SO_RCVBUF).map(int2usize)
    }

    fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        set_opt(self.as_sock(), SOL_SOCKET, SO_SNDBUF, size as c_int)
    }

    fn send_buffer_size(&self) -> io::Result<usize> {
        get_opt(self.as_sock(), SOL_SOCKET, SO_SNDBUF).map(int2usize)
    }

    fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.set_keepalive_ms(keepalive.map(dur2ms))
    }

    fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.keepalive_ms().map(|o| o.map(ms2dur))
    }

    #[cfg(any(unix, target_os = "redox"))]
    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()> {
        set_opt(
            self.as_sock(),
            SOL_SOCKET,
            SO_KEEPALIVE,
            keepalive.is_some() as c_int,
        )?;
        if let Some(dur) = keepalive {
            set_opt(
                self.as_sock(),
                v(IPPROTO_TCP),
                KEEPALIVE_OPTION,
                (dur / 1000) as c_int,
            )?;
        }
        Ok(())
    }

    #[cfg(any(unix, target_os = "redox"))]
    fn keepalive_ms(&self) -> io::Result<Option<u32>> {
        let keepalive = get_opt::<c_int>(self.as_sock(), SOL_SOCKET, SO_KEEPALIVE)?;
        if keepalive == 0 {
            return Ok(None);
        }
        let secs = get_opt::<c_int>(self.as_sock(), v(IPPROTO_TCP), KEEPALIVE_OPTION)?;
        Ok(Some((secs as u32) * 1000))
    }

    #[cfg(windows)]
    fn set_keepalive_ms(&self, keepalive: Option<u32>) -> io::Result<()> {
        let ms = keepalive.unwrap_or(INFINITE);
        let ka = tcp_keepalive {
            onoff: keepalive.is_some() as c_ulong,
            keepalivetime: ms as c_ulong,
            keepaliveinterval: ms as c_ulong,
        };
        unsafe {
            cvt_win(WSAIoctl(
                self.as_sock(),
                SIO_KEEPALIVE_VALS,
                &ka as *const _ as *mut _,
                mem::size_of_val(&ka) as DWORD,
                0 as *mut _,
                0,
                0 as *mut _,
                0 as *mut _,
                None,
            ))
            .map(|_| ())
        }
    }

    #[cfg(windows)]
    fn keepalive_ms(&self) -> io::Result<Option<u32>> {
        let mut ka = tcp_keepalive {
            onoff: 0,
            keepalivetime: 0,
            keepaliveinterval: 0,
        };
        unsafe {
            cvt_win(WSAIoctl(
                self.as_sock(),
                SIO_KEEPALIVE_VALS,
                0 as *mut _,
                0,
                &mut ka as *mut _ as *mut _,
                mem::size_of_val(&ka) as DWORD,
                0 as *mut _,
                0 as *mut _,
                None,
            ))?;
        }
        Ok({
            if ka.onoff == 0 {
                None
            } else {
                timeout2ms(ka.keepaliveinterval as DWORD)
            }
        })
    }

    fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        set_opt(self.as_sock(), SOL_SOCKET, SO_LINGER, dur2linger(dur))
    }

    fn linger(&self) -> io::Result<Option<Duration>> {
        get_opt(self.as_sock(), SOL_SOCKET, SO_LINGER).map(linger2dur)
    }
}

#[cfg(windows)]
fn timeout2ms(dur: DWORD) -> Option<u32> {
    if dur == 0 {
        None
    } else {
        Some(dur)
    }
}

fn linger2dur(linger_opt: linger) -> Option<Duration> {
    if linger_opt.l_onoff == 0 {
        None
    } else {
        Some(Duration::from_secs(linger_opt.l_linger as u64))
    }
}

#[cfg(windows)]
fn dur2linger(dur: Option<Duration>) -> linger {
    match dur {
        Some(d) => linger {
            l_onoff: 1,
            l_linger: d.as_secs() as u16,
        },
        None => linger {
            l_onoff: 0,
            l_linger: 0,
        },
    }
}

#[cfg(any(unix, target_os = "redox"))]
fn dur2linger(dur: Option<Duration>) -> linger {
    match dur {
        Some(d) => linger {
            l_onoff: 1,
            l_linger: d.as_secs() as c_int,
        },
        None => linger {
            l_onoff: 0,
            l_linger: 0,
        },
    }
}

fn ms2dur(ms: u32) -> Duration {
    Duration::new((ms as u64) / 1000, (ms as u32) % 1000 * 1_000_000)
}

fn dur2ms(dur: Duration) -> u32 {
    (dur.as_secs() as u32 * 1000) + (dur.subsec_nanos() / 1_000_000)
}

fn int2usize(n: c_int) -> usize {
    // TODO: casting c_int to a usize should be a checked cast
    n as usize
}

fn cvt<T: utils::One + PartialEq + Neg<Output = T>>(t: T) -> io::Result<T> {
    let one: T = T::one();
    if t == -one {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}

#[cfg(windows)]
fn cvt_win<T: PartialEq + utils::Zero>(t: T) -> io::Result<T> {
    if t == T::zero() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}
