use std::boxed::Box;
use std::ptr;
use std::slice;

use anyhow::Result;
use fuzzmutator::mutator::MutatorEngine;
use rmp_serde::{decode::from_read_ref, Serializer};
use serde::Serialize;

use imgcompress::CompressedBtrfsImage;

struct Mutator {
    engine: MutatorEngine,
    /// We'll return pointers to data in this buffer from `afl_custom_fuzz`
    fuzz_buf: Vec<u8>,
}

impl Mutator {
    fn new() -> Result<Self> {
        Ok(Self {
            engine: MutatorEngine::new()?,
            fuzz_buf: Vec::new(),
        })
    }
}

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
pub extern "C" fn afl_custom_init(
    _afl: *mut libc::c_void,
    _seed: libc::c_uint,
) -> *mut libc::c_void {
    let mutator = match Mutator::new() {
        Ok(m) => m,
        Err(e) => {
            println!("{}", e);
            return ptr::null_mut();
        }
    };

    let boxed = Box::new(mutator);

    Box::into_raw(boxed) as *mut libc::c_void
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
    data: *mut libc::c_void,
    buf: *mut u8,
    buf_size: libc::size_t,
    out_buf: *mut *mut u8,
    _add_buf: *mut u8,
    _add_buf_size: libc::size_t,
    max_size: libc::size_t,
) -> libc::size_t {
    let mutator = unsafe { &mut *(data as *mut Mutator) };

    // Deserialize input
    let serialized: &[u8] = unsafe { slice::from_raw_parts(buf, buf_size) };
    let mut deserialized: CompressedBtrfsImage =
        from_read_ref(&serialized).expect("Failed to deserialize fuzzer input");

    // Mutate payload (but don't touch the metadata)
    mutator.engine.mutate(&mut deserialized.data);
    // The engine shouldn't append any data but it's probably worthwhile to check again
    assert!(deserialized.data.len() + deserialized.metadata.len() <= max_size);

    // Serialize data again
    mutator.fuzz_buf.clear(); // Does not affect capacity
    deserialized
        .serialize(&mut Serializer::new(&mut mutator.fuzz_buf))
        .expect("Failed to serialize fuzzer input");
    assert!(mutator.fuzz_buf.len() <= max_size);

    // Yes, it's ok to hand out ref to the Vec we own. The API is designed this way
    unsafe { out_buf.write(mutator.fuzz_buf.as_mut_ptr()) };

    mutator.fuzz_buf.len()
}

/// Deinitialize everything
///
/// @param data The data ptr from afl_custom_init
#[no_mangle]
pub extern "C" fn afl_custom_deinit(data: *mut libc::c_void) {
    // Reconstruct box and immediately drop to free resources
    unsafe { Box::from_raw(data as *mut Mutator) };
}
