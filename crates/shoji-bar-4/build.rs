//! Picks the interface the binary is built with, from the plugins directory.
//!
//! The plugins directory is `plugins/` at the root of the project (next to the top-level
//! `Cargo.toml`), or `$SHOJI_PLUGINS_DIR` when that is set. In it:
//!
//! - `init.rs` with `pub fn init()` replaces the stock interface: `init` runs as the root of the
//!   shell and puts on the screen whatever it wants.
//! - `init-default.rs` with `pub fn patch(ui: &mut shoji_default::Ui)` keeps the stock interface
//!   and changes it before it is put on the screen.
//! - With neither, the stock interface is used. With both, `init.rs` wins.
//!
//! A file counts only if it defines its function outside of comments: one that is entirely
//! commented out is ignored (with a warning), so commenting a plugin out brings back the stock
//! interface.
//!
//! The chosen file is compiled into this binary as a module (`#[path]`), so the rest of the
//! directory (`mod helpers;` next to `init.rs`) works as in any crate. Editing a file there, or
//! adding or removing one, makes `cargo build` rebuild only this small crate.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn plugins_dir() -> PathBuf {
    if let Some(dir) = env::var_os("SHOJI_PLUGINS_DIR") {
        return PathBuf::from(dir);
    }
    // This crate is at <root>/crates/shoji-bar-4.
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("the crate is two levels below the project root")
        .join("plugins")
}

/// The source with its comments blanked out, so that a function that is only mentioned in a comment
/// is not taken for a definition. Rust block comments nest. Strings are not parsed, which is fine
/// for what this is used for: spotting `pub fn patch(`.
fn without_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut depth) = (0, 0usize);
    while i < bytes.len() {
        if depth == 0 && bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if depth > 0 && bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
        } else {
            if depth == 0 {
                // Copy whole characters, not bytes.
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Whether the file defines `pub fn <name>(` outside its comments.
fn defines(file: &Path, name: &str) -> bool {
    let Ok(src) = fs::read_to_string(file) else {
        return false;
    };
    let code = without_comments(&src);
    let squeezed = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let wanted = format!("pub fn {name}");
    squeezed.match_indices(&wanted).any(|(at, _)| {
        let rest = squeezed[at + wanted.len()..].trim_start();
        rest.starts_with('(') || rest.starts_with('<')
    })
}

fn module(name: &str, file: &Path) -> String {
    // `{:?}` quotes and escapes the path as a Rust string literal.
    format!("#[path = {:?}]\nmod {name};\n", file.display().to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=SHOJI_PLUGINS_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    let dir = plugins_dir();
    // Cargo reruns a build script whose watched path is missing on every build, so the directory
    // is created: an empty one means the stock interface.
    if let Err(e) = fs::create_dir_all(&dir) {
        println!("cargo:warning=cannot create {}: {e}", dir.display());
    }
    println!("cargo:rerun-if-changed={}", dir.display());

    // A file counts when it defines its entry point. One that is all comments (or has had the
    // function taken out) is as if it were not there, with a warning saying so.
    let usable = |file: &Path, entry: &str| {
        if !file.is_file() {
            return false;
        }
        if defines(file, entry) {
            return true;
        }
        println!(
            "cargo:warning={} does not define `pub fn {entry}`: ignoring it",
            file.display()
        );
        false
    };
    let init = dir.join("init.rs");
    let init_default = dir.join("init-default.rs");
    // The modules are named by the directory's real path. The plugins live inside this package
    // (`crates/shoji-bar-4/plugins`, with `plugins/` at the root a symlink to it) because
    // rust-analyzer only loads the files under the directory of a package: with the plugins outside
    // it, they get no completion, hover or go-to-definition. neovim resolves the symlink too, so the
    // file the editor opens is the one rust-analyzer knows. For the compiler, a path is a path.
    let real = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
    let (init_view, init_default_view) = (real.join("init.rs"), real.join("init-default.rs"));
    let use_init = usable(&init, "init");
    let use_init_default = usable(&init_default, "patch");
    let (source, code) = if use_init {
        if use_init_default {
            println!("cargo:warning=init.rs and init-default.rs both exist: using init.rs");
        }
        (
            init.display().to_string(),
            format!(
                "{}\npub fn run() {{\n    shoji_default::run_custom(init::init);\n}}\n",
                module("init", &init_view)
            ),
        )
    } else if use_init_default {
        (
            init_default.display().to_string(),
            format!(
                "{}\npub fn run() {{\n    shoji_default::run_with(init_default::patch);\n}}\n",
                module("init_default", &init_default_view)
            ),
        )
    } else {
        (
            String::new(),
            "pub fn run() {\n    shoji_default::run();\n}\n".to_owned(),
        )
    };

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("plugin_entry.rs");
    let code = format!("pub const SOURCE: &str = {source:?};\n{code}");
    fs::write(out, code).expect("cannot write plugin_entry.rs");
}
