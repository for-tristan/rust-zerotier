//! Linux TAP device allocation.
//!
//! Faithful Rust port of the original `tap_utils.c::tap_alloc`: opens
//! `/dev/net/tun`, configures a TAP device (Ethernet frames, no packet-info
//! header) and returns the kernel-facing file descriptor.
//!
//! Requires Linux with the `tun` module and (at runtime) `CAP_NET_ADMIN`.

use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;

/// `IFF_TAP` — Ethernet device (carries full Ethernet frames).
const IFF_TAP: u16 = 0x0002;
/// `IFF_NO_PI` — do not prepend a 4-byte packet-info header to each frame.
const IFF_NO_PI: u16 = 0x1000;

/// `IFNAMSIZ` — maximum interface name length including the NUL terminator.
const IFNAMSIZ: usize = 16;

/// `TUNSETIFF` ioctl number: `_IOW('T', 202, int)`.
///
/// Encoded with the generic Linux `_IOC` layout used by x86-64 and arm64:
/// `(dir << 30) | (size << 16) | (type << 8) | nr`. The canonical numeric
/// value is `0x400454ca`.
const TUNSETIFF: libc::c_ulong = {
    const IOC_WRITE: libc::c_ulong = 1;
    const TYPE: libc::c_ulong = b'T' as libc::c_ulong;
    const NR: libc::c_ulong = 202;
    const SIZE: libc::c_ulong = std::mem::size_of::<libc::c_int>() as libc::c_ulong;
    (IOC_WRITE << 30) | (SIZE << 16) | (TYPE << 8) | NR
};

/// Minimal `struct ifreq` view: only the name and flags fields are used by
/// `TUNSETIFF`. The remainder is zero-padded to match the kernel's 40-byte
/// layout on x86-64/arm64.
#[repr(C)]
struct Ifreq {
    ifr_name: [u8; IFNAMSIZ],
    ifr_flags: u16,
    _pad: [u8; 22],
}

impl Ifreq {
    const fn zeroed() -> Self {
        Ifreq {
            ifr_name: [0u8; IFNAMSIZ],
            ifr_flags: 0u16,
            _pad: [0u8; 22],
        }
    }
}

/// Allocate a TAP device.
///
/// `requested_name` is used as a *hint* for the interface name (truncated to
/// `IFNAMSIZ - 1` bytes); the kernel may assign a different name, which is
/// returned as the second element of the tuple. The returned file descriptor
/// is used to read/write raw Ethernet frames.
///
/// Errors mirror the original C: a failed `open("/dev/net/tun")` or
/// `ioctl(TUNSETIFF)` (e.g. lack of `CAP_NET_ADMIN`) is reported as the last
/// OS error.
pub fn tap_alloc(requested_name: &str) -> io::Result<(RawFd, String)> {
    // 1. open /dev/net/tun
    let path = CString::new("/dev/net/tun").expect("/dev/net/tun is a valid C string");
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    // 2. build the ifreq: TAP, no packet-info, optional name hint
    let mut ifr = Ifreq::zeroed();
    let name_bytes = requested_name.as_bytes();
    let n = name_bytes.len().min(IFNAMSIZ - 1);
    ifr.ifr_name[..n].copy_from_slice(&name_bytes[..n]);
    ifr.ifr_flags = IFF_TAP | IFF_NO_PI;

    // 3. ioctl(TUNSETIFF)
    let ret = unsafe {
        libc::ioctl(
            fd,
            TUNSETIFF as libc::c_ulong,
            &mut ifr as *mut Ifreq as *mut libc::c_void,
        )
    };
    if ret < 0 {
        let err = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(err);
    }

    // 4. read back the actual interface name (NUL-terminated)
    let actual_name: String = ifr
        .ifr_name
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();

    Ok((fd, actual_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunset_iff_value_is_canonical() {
        // The well-known x86-64/arm64 Linux value for TUNSETIFF.
        assert_eq!(TUNSETIFF, 0x4004_54ca);
    }

    #[test]
    fn ifreq_matches_kernel_layout() {
        assert_eq!(std::mem::size_of::<Ifreq>(), 40);
        assert_eq!(std::mem::offset_of!(Ifreq, ifr_flags), 16);
    }

    #[test]
    fn name_hint_is_truncated_to_ifnamsiz_minus_one() {
        let mut ifr = Ifreq::zeroed();
        let long = "abcdefghijklmnopqrstuvwxyz"; // 26 chars
        let n = long.len().min(IFNAMSIZ - 1);
        ifr.ifr_name[..n].copy_from_slice(&long.as_bytes()[..n]);
        // last slot stays NUL so the kernel sees a terminated string
        assert_eq!(ifr.ifr_name[IFNAMSIZ - 1], 0);
        assert_eq!(&ifr.ifr_name[..5], b"abcde");
    }

    #[test]
    fn flags_combine_tap_and_no_pi() {
        assert_eq!(IFF_TAP | IFF_NO_PI, 0x1002);
    }
}
