#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    zenoh_codec_fuzz::exercise_scouting_message(data);
});
