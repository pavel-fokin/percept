//! Copies `web/dist/index.html` - the page `web/`'s `npm run build`
//! writes - into `OUT_DIR`, so `src/server` can embed it with
//! `include_str!` without Node ever running as part of `cargo build`.
//! A checkout without a built page still compiles: a stub page lands
//! at the same path instead, saying so itself.
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

    let release = env::var("PERCEPT_RELEASE").unwrap_or_else(|_| "dev".to_string());
    let version = match env::var("GITHUB_SHA") {
        Ok(sha) => format!("{release} ({})", sha.get(..7).unwrap_or(&sha)),
        Err(_) => release,
    };
    println!("cargo:rustc-env=PERCEPT_VERSION={version}");
}
