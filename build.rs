//! Copies `web/dist/index.html` - the review page `web/`'s `npm run
//! build` writes - into `OUT_DIR`, so `src/server` can embed it with
//! `include_str!` without Node ever running as part of `cargo build`.
//! A checkout without a built page still compiles: a stub page lands
//! at the same path instead, saying so itself.

use std::env;
use std::fs;
use std::path::Path;

const STUB: &str = "\
<!doctype html>
<html lang=\"en\">
  <head>
    <meta charset=\"UTF-8\" />
    <title>percept review</title>
  </head>
  <body>
    <p>The review page is not built. Run `cd web && npm install && \
npm run build`, then `cargo build`.</p>
  </body>
</html>
";

fn main() {
    let source = Path::new("web/dist/index.html");
    println!("cargo:rerun-if-changed=web/dist");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = Path::new(&out_dir).join("index.html");

    if source.exists() {
        fs::copy(source, &dest).expect("copy web/dist/index.html to OUT_DIR");
    } else {
        fs::write(&dest, STUB).expect("write stub review page to OUT_DIR");
    }
}
