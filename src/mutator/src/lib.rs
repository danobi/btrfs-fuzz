/// Initialize this custom mutator
///
/// @param[in] afl a pointer to the internal state object. Can be ignored for
/// now.
/// @param[in] seed A seed for this mutator - the same seed should always mutate
/// in the same way.
/// @return Pointer to the data object this custom mutator instance should use.
///         There may be multiple instances of this mutator in one afl-fuzz run!
///         Return NULL on error.
#[no_mangle]
pub extern "C" fn afl_custom_init(_afl: *mut libc::c_void, _seed: libc::c_uint) -> *mut libc::c_void {
    unimplemented!();
}

/// Perform custom mutations on a given input
///
/// @param[in] data pointer returned in afl_custom_init for this fuzz case
/// @param[in] buf Pointer to input data to be mutated
/// @param[in] buf_size Size of input data
/// @param[out] out_buf the buffer we will work on. we can reuse *buf. NULL on
/// error.
/// @param[in] add_buf Buffer containing the additional test case
/// @param[in] add_buf_size Size of the additional test case
/// @param[in] max_size Maximum size of the mutated output. The mutation must not
///     produce data larger than max_size.
/// @return Size of the mutated output.
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

/// Deinitialize everything
///
/// @param data The data ptr from afl_custom_init
#[no_mangle]
pub extern "C" fn afl_custom_deinit(_data: *mut libc::c_void) {
    unimplemented!();
}
