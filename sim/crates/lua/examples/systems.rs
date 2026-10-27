use std::path::Path;
fn main() {
    let rt = lua::Runtime::new(Path::new(r"E:\metrostroi2pz\src\scripts\lua")).unwrap();
    let (ok, bad) = rt.load_systems();
    println!("loaded {ok}, failed {}", bad.len());
    for (f, e) in bad { println!("{f}: {e}"); }
}
