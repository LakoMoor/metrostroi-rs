use std::path::Path;
fn main() {
    let rt = lua::Runtime::new(Path::new(r"E:\metrostroi2pz\src\scripts\lua")).unwrap();
    rt.include("autorun/metrostroi.lua").unwrap();
    let t = rt.spawn_train("gmod_subway_81-717_mvm").unwrap();
    rt.lua.globals().set("T", t).unwrap();
    let script = r#"
        local function tick(n)
            for i = 1, n do
                __time = __time + 1/60
                local ok, err = pcall(T.Think, T)
                if not ok then print("Think error: " .. tostring(err)) return false end
            end
            return true
        end
        local function show(tag)
            local e = T.Electric
            print(tag, "Battery80V=", e and e.Battery80V, "Main750V=", e and e.Main750V, "V=", T.Speed)
        end
        tick(60) show("idle")
        T:ButtonEvent("VBToggle", true) tick(5) T:ButtonEvent("VBToggle", false)
        tick(120) show("after VB")
        print("VB.Value=", T.VB and T.VB.Value)
        local function st(tag) print(tag, "Bat V=", T.Battery.Voltage, "Aux80V=", T.Electric.Aux80V, "CabLights=", T.Panel.CabLights, "Headlights1=", T.Panel.Headlights1) end
        st("VB on")
        T:ButtonEvent("VBToggle", true) tick(5) T:ButtonEvent("VBToggle", false) tick(120) st("VB off again")
        T:ButtonEvent("VBToggle", true) tick(5) T:ButtonEvent("VBToggle", false) tick(240) st("VB on")
    "#;
    if let Err(e) = rt.exec(script, "@tick") { println!("{e:#}"); }
}
