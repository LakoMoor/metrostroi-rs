use formats::bsp;
fn main() -> anyhow::Result<()> {
    let data = std::fs::read(r"E:\metrostroi2pz\game\content\maps\crossline_c4\map.bsp")?;
    let txt = std::fs::read_to_string(r"E:\metrostroi2pz\game\content\maps\crossline_c4\track.lua")?;
    let mut nodes = vec![];
    for l in txt.lines() {
        if let Some(a) = l.find("\"[") {
            let s = &l[a + 2..];
            let s = &s[..s.find(']').unwrap()];
            let v: Vec<f32> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() == 3 { nodes.push(v); }
        }
    }
    let meshes = bsp::load(&data)?;
    let mut tris = vec![];
    for m in &meshes {
        for t in m.indices.chunks(3) {
            let p: Vec<[f32; 3]> = t.iter().map(|&i| m.vertices[i as usize].pos).collect();
            tris.push([p[0], p[1], p[2]]);
        }
    }
    let mut hist = std::collections::BTreeMap::new();
    for n in nodes.iter().step_by(3) {
        for t in &tris {
            // point in triangle (xy), then plane z
            let (a, b, c) = (t[0], t[1], t[2]);
            let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if d.abs() < 1e-3 { continue; }
            let l1 = ((b[1] - c[1]) * (n[0] - c[0]) + (c[0] - b[0]) * (n[1] - c[1])) / d;
            let l2 = ((c[1] - a[1]) * (n[0] - c[0]) + (a[0] - c[0]) * (n[1] - c[1])) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 { continue; }
            let z = l1 * a[2] + l2 * b[2] + l3 * c[2];
            let dz = z - n[2];
            if dz.abs() < 120.0 { *hist.entry((dz / 2.0).round() as i32 * 2).or_insert(0) += 1; }
        }
    }
    println!("{hist:?}");
    Ok(())
}
