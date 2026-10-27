use std::path::Path;
fn main() {
    let rt = lua::Runtime::new(Path::new(r"E:\metrostroi2pz\src\scripts\lua")).unwrap();
    rt.include("autorun/metrostroi.lua").unwrap();
    let a = std::env::args().nth(1).unwrap();
    if let Err(e) = rt.exec(&a, "@probe") { println!("{e:#}"); }
}
