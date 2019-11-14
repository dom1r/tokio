pub use winapi::ctypes::{c_int, c_ulong, c_void};
pub use winapi::shared::minwindef::DWORD;
pub use winapi::shared::ws2def::{IPPROTO, IPPROTO_TCP, TCP_NODELAY};
pub use winapi::um::winbase::INFINITE;
pub use winapi::um::winsock2::{getsockopt, linger, setsockopt, WSAIoctl, SOCKET, SOL_SOCKET};
pub use winapi::um::winsock2::{SO_LINGER, SO_RCVBUF, SO_SNDBUF};
pub use winapi::um::ws2tcpip::socklen_t;
