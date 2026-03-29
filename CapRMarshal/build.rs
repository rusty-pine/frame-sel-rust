// build.rs
//
// Generates Rust bindings from the Cap'n Proto schema.
//
// Key fixes vs. previous revision:
//
//  • Output path is now $OUT_DIR (the Cargo-managed build artifact directory)
//    rather than src/.  Writing generated files into src/ pollutes version
//    control, breaks incremental builds (timestamp changes every build), and
//    mixes generated and human-authored code (issue #4 in Phase 1 review).
//
//  • println!("cargo:rerun-if-changed=...") added so Cargo only re-runs this
//    script when the schema actually changes, not on every incremental build
//    (issue #25 in Phase 1 review).
//
//  • Uses the `capnpc` crate (build-dependency), not `capnp`.  The capnp
//    crate is the runtime; capnpc is the code-generator.  Without capnpc in
//    [build-dependencies] this file fails to compile (issue #5 in Phase 1
//    review).
//
// In lib.rs, include the generated file with:
//
//   pub mod schemas {
//       include!(concat!(env!("OUT_DIR"), "/bridge_capnp.rs"));
//   }

fn main() -> std::io::Result<()> {
    // Only re-run if the schema or this build script changes.
    println!("cargo:rerun-if-changed=src/schemas/bridge.capnp");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR")
        .expect("Cargo must set OUT_DIR before invoking build.rs");

    capnpc::CompilerCommand::new()
        .file("src/schemas/bridge.capnp")
        .output_path(&out_dir)
        .run()
        .expect("capnp codegen failed — ensure `capnp` compiler is installed");

    Ok(())
}
