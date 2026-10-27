use formats::gma::Gma;
use std::path::Path;
// extract <gma> <entry-substring> <out-file>
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let g = Gma::open(Path::new(&a[0])).unwrap();
    let e = g.entries.iter().find(|e| e.path.contains(a[1].as_str())).expect("entry not found");
    std::fs::write(&a[2], g.read(e)).unwrap();
    println!("{} -> {} ({} bytes)", e.path, a[2], g.read(e).len());
}
