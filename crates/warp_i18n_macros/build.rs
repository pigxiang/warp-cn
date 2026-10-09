//! Build-time key index generator for the `t!()` proc-macro.
//!
//! Locates the sibling `crates/warp_i18n/bundles/en/` tree, parses every `.ftl`, and
//! emits `OUT_DIR/keys.rs` containing a `phf::Set<&'static str>`. The proc-macro
//! `include!`s this set at expansion time to validate literal keys.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    // crates/warp_i18n_macros -> crates/warp_i18n
    let bundles_en = manifest_dir
        .parent()
        .unwrap()
        .join("warp_i18n")
        .join("bundles")
        .join("en");
    println!("cargo:rerun-if-changed={}", bundles_en.display());

    let mut keys: BTreeSet<String> = BTreeSet::new();
    if bundles_en.is_dir() {
        for entry in walkdir::WalkDir::new(&bundles_en).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) != Some("ftl") {
                continue;
            }
            println!("cargo:rerun-if-changed={}", entry.path().display());
            let source = match fs::read_to_string(entry.path()) {
                Ok(s) => s,
                Err(e) => panic!("warp_i18n_macros: read {}: {e}", entry.path().display()),
            };
            match fluent_syntax::parser::parse(source.as_str()) {
                Ok(resource) => {
                    for ast_entry in resource.body {
                        if let fluent_syntax::ast::Entry::Message(msg) = ast_entry {
                            keys.insert(msg.id.name.to_string());
                        }
                    }
                }
                Err((_, errs)) => {
                    panic!(
                        "warp_i18n_macros: ftl parse error(s) in {}: {:?}",
                        entry.path().display(),
                        errs
                    );
                }
            }
        }
    } else {
        // Bundles dir absent during the very first scaffolding compile; emit an empty set
        // so the macro crate still builds. Compile errors will then surface the moment
        // someone calls `t!("...")` with no keys defined.
        println!(
            "cargo:warning=warp_i18n_macros: bundles dir {} not found, emitting empty key set",
            bundles_en.display()
        );
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let dest = out_dir.join("keys.rs");
    let mut file = fs::File::create(&dest).expect("create keys.rs");
    let mut builder = phf_codegen::Set::new();
    for k in &keys {
        builder.entry(k.as_str());
    }
    writeln!(
        file,
        "static EN_KEY_INDEX: phf::Set<&'static str> = {};",
        builder.build()
    )
    .unwrap();
}
