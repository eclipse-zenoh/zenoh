#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    zenoh_protocol_fuzz::exercise_endpoint_from_str(data);
});
