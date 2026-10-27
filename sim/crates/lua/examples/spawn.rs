use std::path::Path;
fn main() {
    let rt = lua::Runtime::new(Path::new(r"E:\metrostroi2pz\src\scripts\lua")).unwrap();
    if let Err(e) = rt.include("autorun/metrostroi.lua") { println!("autorun: {e:#}"); return; }
    match rt.spawn_train("gmod_subway_81-717_mvm") {
        Ok(_) => println!("spawned"),
        Err(e) => println!("spawn failed: {e:#}"),
    }
}
