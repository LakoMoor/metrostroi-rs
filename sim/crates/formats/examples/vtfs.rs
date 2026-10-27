use formats::{vfs::Vfs, vtf};
use std::{collections::BTreeMap, path::Path};

fn main() -> anyhow::Result<()> {
    let ws = Path::new(r"E:\Games\steamapps\workshop\content\4000");
    let mut vfs = Vfs::default();
    for id in ["1812142201", "1095094174", "1095098251", "1095100683", "1095105863", "1095109617", "1095111608"] {
        let g = std::fs::read_dir(ws.join(id))?.filter_map(|e| e.ok()).map(|e| e.path())
            .find(|p| p.extension().map_or(false, |x| x == "gma")).unwrap();
        vfs.mount(&g)?;
    }
    let (mut ok, mut bad) = (0, BTreeMap::<String, u32>::new());
    let t = std::time::Instant::now();
    let mut sample = None;
    for p in vfs.paths().filter(|p| p.ends_with(".vtf")).map(|s| s.to_string()).collect::<Vec<_>>() {
        match vtf::decode(vfs.read(&p).unwrap()) {
            Ok(v) => { ok += 1; if p.contains("717_classic1") { sample = Some((p.clone(), v)); } }
            Err(e) => *bad.entry(e.to_string()).or_default() += 1,
        }
    }
    println!("decoded {ok} VTFs in {:?}; failures: {bad:?}", t.elapsed());
    if let Some((p, v)) = sample {
        println!("{p}: {}x{} mips={} alpha={}", v.width, v.height, v.mips.len(), v.has_alpha);
        // dump mip 0 as PPM so it can be checked by eye
        let mut ppm = format!("P6\n{} {}\n255\n", v.width, v.height).into_bytes();
        for px in v.mips[0].chunks_exact(4) { ppm.extend_from_slice(&px[..3]); }
        std::fs::write(r"E:\metrostroi2pz\out\sample_717_classic1.ppm", ppm)?;
    }
    Ok(())
}
