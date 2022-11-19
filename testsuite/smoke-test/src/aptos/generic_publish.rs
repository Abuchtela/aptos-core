// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use framework::{BuildOptions, BuiltPackage};
use move_binary_format::CompiledModule;
use std::{fs::File, io::Write};

// Update `raw_module_data.rs` in
// `crates/transaction-emitter-lib/src/transaction_generator/publishing/`.
// That file contains `Lazy` static variables for the binary of all the modules in
// `testsuit/smoke-test/src/aptos/module_publishing/` as `Lazy`.
// In `crates/transaction-emitter-lib/src/transaction_generator/publishing` you should
// also find the files that can load, manipulate and use the modules.
// Typically those modules will be altered (publishing at different addresses requires a module
// address rewriting, versioning may benefit from real changes), published and used in transaction.
// Code to conveniently do that should be in that crate.
//
// All of that considered, please be careful when changing this file or the modules in
// `testsuit/smoke-test/src/aptos/module_publishing/` given that it will likely require
// changes in `crates/transaction-emitter-lib/src/transaction_generator/publishing`.
#[ignore]
#[test]
fn publish_for_emitter() {
    // build GenericModule
    let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // this is gotta be the most brittle solution ever!
    // If directory structure changes this breaks.
    // However it is a test that is ignored and runs only with the intent of creating files
    // for the modules compiled, so people can change it as they wish and need to.
    let base_path =
        base_dir.join("../../crates/transaction-emitter-lib/src/transaction_generator/publishing/");
    let mut generic_mod = std::fs::File::create(&base_path.join("raw_module_data.rs")).unwrap();

    //
    // File header
    //
    writeln!(
        generic_mod,
        r#"// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0"#
    )
    .expect("Writing header comment failed");

    //
    // Module comment
    //
    writeln!(
        generic_mod,
        r#"
// This file was generated. Do not modify!
//
// To update this code, run `cargo test publish_for_emitter -- --ignore`.
// from `testsuite/smoke-test` in aptos core.
// That test compiles the set of modules defined in
// `testsuite/smoke-test/src/aptos/module_publishing/sources/`
// and it writes the binaries here.
// The module name (prefixed with `MODULE_`) is a `Lazy` instance that returns the
// byte array of the module binary.
// This create should also provide a Rust file that allows proper manipulation of each
// module defined below."#
    )
    .expect("Writing header comment failed");

    //
    // use ... directives
    //
    writeln!(
        generic_mod,
        r#"
use once_cell::sync::Lazy;
"#,
    )
    .expect("Use directive failed");

    // write out package metadata
    write_pacakge_simple(&mut generic_mod);
}

// Write out package `Simple`
fn write_pacakge_simple(file: &mut File) {
    // build GenericModule
    let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = base_dir.join("src/aptos/module_publishing/");
    let package =
        BuiltPackage::build(path, BuildOptions::default()).expect("building package must succeed");
    let code = package.extract_code();
    let package_metadata = package.extract_metadata().expect("Metadata must exist");
    let metadata = bcs::to_bytes(&package_metadata).expect("Metadata must serialize");

    // write out package metadata
    write_lazy(file, "PACKAGE_METADATA_SIMPLE", &metadata);

    // write out all modules
    for module in &code {
        // this is an unfortunate way to find the module name but it is not
        // clear how to do it otherwise
        let compiled_module = CompiledModule::deserialize(module).expect("Module must deserialize");
        let module_name = compiled_module.self_id().name().to_owned().into_string();
        // start Lazy declaration
        let name = format!("MODULE_{}", module_name.to_uppercase());
        writeln!(file).expect("Empty line failed");
        write_lazy(file, name.as_str(), module);
    }
}

// Write out a `Lazy` declaration
fn write_lazy(file: &mut File, data_name: &str, data: &[u8]) {
    writeln!(file, "#[rustfmt::skip]").expect("rustfmt skip failed");
    writeln!(
        file,
        "pub static {}: Lazy<Vec<u8>> = Lazy::new(|| {{",
        data_name,
    )
    .expect("Lazy declaration failed");
    write_vector(file, data);
    writeln!(file, "}});").expect("Lazy declaration closing } failed");
}

// count of elements on a single line
const DATA_BREAK_UP: usize = 18;

// Write out a vector of bytes
fn write_vector(file: &mut File, data: &[u8]) {
    writeln!(file, "\tvec![").expect("Vector header failed");
    write!(file, "\t\t").expect("Tab write failed");
    let mut newline = false;
    for (idx, datum) in data.iter().enumerate() {
        if (idx + 1) % DATA_BREAK_UP == 0 {
            writeln!(file, "{},", datum).expect("Vector write failed");
            write!(file, "\t\t").expect("Tab write failed");
            newline = true;
        } else {
            if idx == data.len() - 1 {
                write!(file, "{},", datum).expect("Vector write failed");
            } else {
                write!(file, "{}, ", datum).expect("Vector write failed");
            }
            newline = false;
        }
    }
    if !newline {
        writeln!(file).expect("Empty writeln failed");
    }
    writeln!(file, "\t]").expect("Vector footer failed");
}
