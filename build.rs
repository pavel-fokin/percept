use std::{env, fs, path::Path};

const STUB: &str = "<!doctype html><title>percept</title>\
<p>The page is not built. Run <code>cd web &amp;&amp; npm install &amp;&amp; npm run build</code>, then <code>cargo build</code>.</p>";

fn main() {
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("index.html");
    let page = fs::read_to_string("web/dist/index.html").unwrap_or_else(|_| STUB.to_string());
    fs::write(out, page).unwrap();
    println!("cargo:rerun-if-changed=web/dist");
}
