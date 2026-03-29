// fuzz/fuzz_targets/fuzz_capnp_parse.rs
//
// cargo-fuzz target: exercises the Cap'n Proto deserialisation + dispatch
// path with arbitrary byte input.
//
// Run locally:
//   cargo +nightly fuzz run fuzz_capnp_parse
//
// The fuzzer looks for inputs that cause panics, assertion failures, or
// memory safety violations.  All `?` paths in the dispatch function return
// typed errors — the fuzzer is looking for any *uncaught* panic.

#![no_main]
use libfuzzer_sys::fuzz_target;
use capnp::message::ReaderOptions;
use capnp::serialize;

// Import the generated Cap'n Proto types.
use capability_marshaling::schemas::bridge_capnp::capability_bridge;

fuzz_target!(|data: &[u8]| {
    let mut slice = data;

    // Attempt to parse the input as a Cap'n Proto message.
    // If parsing fails, that is an expected outcome — not a crash.
    let Ok(reader) = serialize::read_message_from_flat_slice(
        &mut slice,
        ReaderOptions::new(),
    ) else {
        return;
    };

    let Ok(bridge) = reader.get_root::<capability_bridge::Reader>() else {
        return;
    };

    // Exercise every dispatch branch without panicking.
    match bridge.which() {
        Err(_) => {}
        Ok(capability_bridge::Which::OpenSession(params)) => {
            if let Ok(p) = params {
                let _ = p.get_client_id();
            }
        }
        Ok(capability_bridge::Which::PreGrantMemory(params)) => {
            if let Ok(p) = params {
                if let Ok(bundle) = p.get_bundle() {
                    let _ = bundle.get_base_paddr();
                    let _ = bundle.get_num_frames();
                }
            }
        }
    }
    // If we reach here without panicking, the fuzz run succeeds.
});
