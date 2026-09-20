//! Copies `web/dist/index.html` - the page `web/`'s `npm run build`
//! writes - into `OUT_DIR`, so `src/server` can embed it with
//! `include_str!` without Node ever running as part of `cargo build`.
//! A checkout without a built page still compiles: a stub page lands
//! at the same path instead, saying so itself.
//!
//! Also generates the list of shipped schema templates
//! `mapstore::templates` embeds, so a new or renamed file under
//! `src/mapstore/schemas` needs no matching change in Rust source -
//! see `generate_schema_templates`.
//!
//! Also stamps in the release number `percept --version` prints.

use std::env;
use std::fs;
use std::path::Path;

const STUB: &str = "\
<!doctype html>
<html lang=\"en\">
  <head>
    <meta charset=\"UTF-8\" />
    <title>percept</title>
  </head>
  <body>
    <p>The page is not built. Run `cd web && npm install && \
npm run build`, then `cargo build`.</p>
  </body>
</html>
";

fn main() {
    stamp_version();
    generate_schema_templates();

    let source = Path::new("web/dist/index.html");
    println!("cargo:rerun-if-changed=web/dist");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = Path::new(&out_dir).join("index.html");

    if source.exists() {
        fs::copy(source, &dest).expect("copy web/dist/index.html to OUT_DIR");
    } else {
        fs::write(&dest, STUB).expect("write stub page to OUT_DIR");
    }
}

/// Emits `PERCEPT_VERSION` for `percept --version`: the release number
/// CI is publishing under and the commit it built, or `dev` in a
/// checkout, where no number has been published to name.
fn stamp_version() {
    println!("cargo:rerun-if-env-changed=PERCEPT_RELEASE");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");

    let release = env_value("PERCEPT_RELEASE").unwrap_or_else(|| "dev".to_string());
    let version = match env_value("GITHUB_SHA") {
        Some(sha) => format!("{release} ({})", sha.get(..7).unwrap_or(&sha)),
        None => release,
    };
    println!("cargo:rustc-env=PERCEPT_VERSION={version}");
}

/// Every `*.toml` file directly under `src/mapstore/schemas`, sorted,
/// as one `(<stem>, include_str!(...))` pair each in a generated
/// `TEMPLATE_TEXTS` const - a schema's name is its file's stem, so the
/// file list lives here, discovered, and never as a name a person
/// keeps in sync by hand.
fn generate_schema_templates() {
    let dir = Path::new("src/mapstore/schemas");
    println!("cargo:rerun-if-changed={}", dir.display());

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let mut files: Vec<_> = fs::read_dir(dir)
        .expect("read src/mapstore/schemas")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();

    let mut code = String::from("pub const TEMPLATE_TEXTS: &[(&str, &str)] = &[\n");
    for file in &files {
        let stem = file.file_stem().expect("schema file has a stem").to_string_lossy();
        let absolute = Path::new(&manifest_dir).join(file);
        code.push_str(&format!("    ({stem:?}, include_str!({absolute:?})),\n"));
    }
    code.push_str("];\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    fs::write(Path::new(&out_dir).join("schema_templates.rs"), code)
        .expect("write schema_templates.rs");
}

/// A variable set to the empty string is as absent as an unset one.
/// `PERCEPT_RELEASE` arrives that way if the job output it comes from
/// never got written, and would stamp a blank where the number goes.
fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}
