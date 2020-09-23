use libc;

#[no_mangle]
pub extern "C" fn afl_custom_fuzz(
    _data: *mut libc::c_void,
    _buf: *mut u8,
    _buf_size: u8,
    _out_buf: *mut *mut u8,
    _add_buf: *mut u8,
    _add_buf_size: libc::size_t,
    _max_size: libc::size_t,
) -> libc::size_t {
    unimplemented!();
}
