use formats::vfs::Vfs;
use std::collections::BTreeMap;
use std::path::Path;
// usage: du <workshop dir> <id>... — bytes per top-level / second-level directory over all mounted GMAs
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut vfs = Vfs::default();
    for id in &a[1..] {
        let dir = Path::new(&a[0]).join(id);
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "gma" || x == "bin") {
                vfs.mount(&e.path()).unwrap();
            }
        }
    }
    let mut by: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let paths: Vec<String> = vfs.paths().map(|s| s.to_string()).collect();
    for p in paths {
        let n = vfs.read(&p).map_or(0, |d| d.len());
        let k = p.split('/').take(2).collect::<Vec<_>>().join("/");
        let e = by.entry(k).or_default();
        e.0 += 1;
        e.1 += n;
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1 .1));
    for (k, (n, b)) in v.iter().take(40) {
        println!("{:>8.1} MB {:>6} files  {k}", *b as f64 / 1e6, n);
    }
    println!("total {:.1} MB", v.iter().map(|x| x.1 .1).sum::<usize>() as f64 / 1e6);
}
