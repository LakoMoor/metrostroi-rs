//! The running train: Metrostroi's own Lua systems, ticked every frame.

use anyhow::Result;
use mlua::Value;
use crate::dynamics::{BogeyIn, Dynamics};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const LUA_ROOT: &str = r"E:\metrostroi2pz\src\scripts\lua";

const HELPERS: &str = r#"
    C = Metrostroi.CreateClient(T)
    -- the cab's button panels as the Lua defines them (positions already evaluated)
    function __panels()
        local r = {}
        for id, panel in pairs(C.ButtonMap) do
            if type(panel) == "table" and panel.buttons and panel.pos and panel.ang then
                local bl = {}
                for _, b in ipairs(panel.buttons) do
                    bl[#bl + 1] = { ID = b.ID, x = b.x, y = b.y, w = b.w, h = b.h, radius = b.radius, tooltip = b.tooltip }
                end
                r[#r + 1] = { name = id, pos = { x = panel.pos.x, y = panel.pos.y, z = panel.pos.z },
                    ang = { p = panel.ang.p, y = panel.ang.y, r = panel.ang.r },
                    scale = panel.scale, width = panel.width, height = panel.height, buttons = bl }
            end
        end
        return r
    end
    function __props()
        local r = {}
        for name, p in pairs(C.ClientProps) do
            if type(p) == "table" and type(p.model) == "string" and p.model ~= "" and p.pos then
                local a = p.ang or Angle(0, 0, 0)
                r[#r + 1] = { name, p.model, p.pos.x, p.pos.y, p.pos.z, a.p, a.y, a.r }
            end
        end
        table.sort(r, function(x, y) return x[1] < y[1] end)
        return r
    end
    -- visible client props: name -> pose "position" value, or true when it has none
    function __state()
        local r = {}
        for name, e in pairs(C.ClientEnts) do
            if type(e) == "table" and e.IsValid and e:IsValid() and not e.__nodraw then
                local c = e.__color
                r[name] = { e.__pose and e.__pose.position or false, c and c.a or 255 }
            end
        end
        return r
    end
    -- texture basename -> replacement material, as the cab's UpdateTextures() would apply (cab < pass < train)
    function __skin_map()
        local out = {}
        for _, kind in ipairs({ { "cab", "CabTexture" }, { "pass", "PassTexture" }, { "train", "Texture" } }) do
            local skins = Metrostroi.Skins and Metrostroi.Skins[kind[1]]
            local sk = skins and skins[T:GetNW2String(kind[2])]
            if sk and sk.textures then
                for name, path in pairs(sk.textures) do
                    if type(path) == "string" then out[name:lower()] = path end
                end
            end
        end
        return out
    end
    -- keyboard: GMod key name ("A", "PAD_1", "LSHIFT"...) goes through the entity's own KeyMap handling
    function __key(name, down)
        local k = _G["KEY_" .. name]
        if k then
            local ok, err = pcall(T.OnKeyEvent, T, k, down, nil)
            if not ok then return tostring(err) end
        end
    end
    -- buttons the cab currently hides (same test as the cab's own mouse code)
    -- Metrostroi's failure simulator (the original ages objects in Turbostroi's C++ side)
    FAIL_AUTO = false
    function __fail_age(dt)
        if not (FAIL_AUTO and FailSim and FailSim.Objects) then return end
        for obj in pairs(FailSim.Objects) do FailSim.Age(obj, dt) end
    end
    function __fail_now()
        if not (FailSim and FailSim.RandomFailure) then return nil end
        local out, p = nil, print
        print = function(...)
            local t = {}
            for i, v in ipairs({...}) do t[i] = tostring(v) end
            local s = table.concat(t, " ")
            if s:find("[!FAIL!]", 1, true) then out = s end
        end
        local ok = pcall(FailSim.RandomFailure)
        print = p
        return ok and out or nil
    end
    function __fail_list()
        local r = {}
        for obj, o in pairs(FailSim.Objects or {}) do
            for _, prm in pairs(o.Parameters) do
                for _, f in pairs(prm.failures or {}) do r[#r + 1] = f.name .. " (" .. tostring(obj.Name or "?") .. ")" end
            end
        end
        return r
    end
    function __fail_reset() if FailSim and FailSim.Reset then FailSim.Reset() end end
    function __hidden_buttons()
        local H, out = C.Hidden, {}
        for _, panel in pairs(C.ButtonMap) do
            if type(panel) == "table" and panel.buttons then
                for _, b in ipairs(panel.buttons) do
                    local pn = b.PropName
                    if H[b.ID] or H.button[b.ID] or H.anim[b.ID] or (pn and (H[pn] or H.button[pn] or H.anim[pn])) then
                        out[#out + 1] = b.ID
                    end
                end
            end
        end
        return out
    end
    -- bogey inputs for the Rust dynamics: motor force/power, reversed, brake force/cylinder/parking/pow
    function __phys_in()
        local r = {}
        local list = {}
        for _, c in ipairs(__cars) do list[#list + 1] = c.FrontBogey list[#list + 1] = c.RearBogey end
        for i, b in ipairs(list) do
            r[i] = { b.MotorForce or 30000, b.MotorPower or 0, b.Reversed and 1 or 0, b.PneumaticBrakeForce or 100000,
                     b.BrakeCylinderPressure or 0, b.ParkingBrakePressure or 0, b.PneumaticPow or 1 }
        end
        return r
    end
    function __phys_out(o)
        local i = 1
        for _, c in ipairs(__cars) do
            for _, b in ipairs({ c.FrontBogey, c.RearBogey }) do
                b.Speed, b.SpeedSign, b.Acceleration = o[i], o[i + 1], o[i + 2]
                i = i + 3
            end
        end
    end
    -- Stand-in for the rail network's ARS section under the train: Rust fills in the code of the next signal.
    ARS_SIGNAL = { Name = "?", Red = false, ARSSpeedLimit = nil, TwoToSix = false, Approve0 = false }
    function ARS_SIGNAL:IsValid() return true end
    function ARS_SIGNAL:GetARS(id, f15, f26)
        if not self.ARSSpeedLimit then return false end
        return self.ARSSpeedLimit == id
    end
    function ARS_SIGNAL:GetRS() return false end
    function ARS_SIGNAL:Get325HzAproove0() return false end
    Metrostroi.TrainPositions = setmetatable({}, { __index = function() return { { node1 = {}, x = 0 } } end })
    Metrostroi.TrainDirections = setmetatable({}, { __index = function() return true end })
    Metrostroi.GetARSJoint = function() return ARS_SIGNAL, nil end
    function __ars(code, name)
        ARS_SIGNAL.ARSSpeedLimit = code
        ARS_SIGNAL.Name = name or "?"
        ARS_SIGNAL.Red = code == 0
    end
    -- scripted cab start-up (the procedure from the `drive_and_stop` test), run as a coroutine inside the tick
    local function wait(sec)
        local t = __time + sec
        while __time < t do coroutine.yield() end
    end
    function __quickstart()
        __qs = coroutine.create(function()
            for _, n in ipairs({ "VB", "VAH", "VAD" }) do
                if T[n] and T[n].Value < 0.5 then T:ButtonEvent(n .. "Toggle", true) T:ButtonEvent(n .. "Toggle", false) end
            end
            for _, id in ipairs({ "DriverValveBLDisconnectToggle", "DriverValveTLDisconnectToggle", "DriverValveDisconnectToggle" }) do
                T:ButtonEvent(id, true) T:ButtonEvent(id, false)
            end
            for _, k in ipairs({ "PAD_ENTER", "PAD_PLUS", "PAD_1" }) do __key(k, true) wait(0.02) __key(k, false) end
            wait(10)
            -- every key goes up again, or the cab would ignore the player's next press of it
            __key("5", true) wait(3) __key("5", false)
            __key("4", true) T:ButtonEvent("VozvratRPSet", true) wait(0.5) T:ButtonEvent("VozvratRPSet", false) __key("4", false)
            __key("5", true) wait(2) __key("5", false)
            __key("4", true) wait(0.1) __key("4", false)
        end)
    end
    function __qs_active() return __qs ~= nil and coroutine.status(__qs) ~= "dead" end
    RAIL_OHMS = 0.03
    -- the real consist: more trains linked front-to-rear so Metrostroi's own train-wire code (WagonList, leader) runs
    __cars = { T }
    function __couple(n)
        for i = #__cars, 2, -1 do __cars[i] = nil end
        for i = 2, n do __cars[i] = Metrostroi.SpawnTrain("gmod_subway_81-714_mvm") end
        for i, t in ipairs(__cars) do
            t.FrontTrain = __cars[i - 1]
            t.RearTrain = __cars[i + 1]
            t.TrainCoupledIndex = 1
            t.WagonList = nil
        end
        for i, t in ipairs(__cars) do
            t:UpdateWagonList()
            t.TrainWireLeader = i == 1
        end
    end
    function __tick(dt)
        __time = __time + dt
        if __qs_active() then
            local ok, err = coroutine.resume(__qs)
            if not ok then print("quickstart: " .. tostring(err)) __qs = nil end
        end
        -- third-rail voltage comes from Rust (Sim::rail_voltage)
        -- the line sags under load: feeder resistance times the traction current (eased like the real contact patch)
        local amps = math.abs((T.Electric and T.Electric.Itotal) or 0)
        __sag = (__sag or 0) + (amps * RAIL_OHMS - (__sag or 0)) * math.min(1, dt * 3)
        local volts = math.max(0, __rail - (__rail > 0 and __sag or 0))
        T.FrontBogey.Voltage = volts
        T.RearBogey.Voltage = volts
        for i = 2, #__cars do
            local c = __cars[i]
            c.FrontBogey.Voltage = volts
            c.RearBogey.Voltage = volts
            net.Muted = true -- only the driven car is heard and animated
            local okc, errc = pcall(c.Think, c)
            net.Muted = false
            if not okc then return "car " .. i .. ": " .. tostring(errc) end
        end
        local ok, err = pcall(T.Think, T)
        if not ok then return tostring(err) end
        C.RingVolume = C.RingVolume or 1
        C.RingPitch = C.RingPitch or 1
        C.RingFade = C.RingFade or 0
        ok, err = pcall(Metrostroi.ClientThink, C, dt)
        if not ok then return "client: " .. tostring(err) end
    end
    -- sounds: the client's SetSoundState / PlayOnce are collected here and played by Rust (audio.rs)
    local SND = { loops = {}, once = {} }
    local function sound_files(id)
        local n = C.SoundNames and C.SoundNames[id]
        if type(n) == "string" then return { n } end
        if type(n) == "table" then
            local t = {}
            for i, v in ipairs(n) do t[i] = v end
            return t
        end
    end
    function C.SetSoundState(self, id, vol, pitch) if id then SND.loops[id] = { vol or 0, pitch or 1 } end end
    local function once(self, id, loc, vol, pitch)
        -- switches, valves and the like map to sounds by their client definition (ENT.ClientSounds, see shared.lua PlayOnce)
        local cs = C.ClientSounds and C.ClientSounds[id]
        if cs then
            for _, e in ipairs(cs) do
                local ok, sid = pcall(e[2], C, vol or 0, loc)
                local f = ok and sid and sound_files(sid)
                if f then SND.once[#SND.once + 1] = { f, e[3] or 1, (e[4] or 1) * (pitch or 1) } end
            end
            return
        end
        local f = sound_files(id)
        if f and (vol or 0.8) > 0 then SND.once[#SND.once + 1] = { f, vol or 0.8, pitch or 1 } end
    end
    C.PlayOnce = once
    T.PlayOnce = once
    -- the announcer (UPO/ASNP) plays whole files by name on the train entity
    T.RenderClientEnts = true
    function T.PlayOnceFromPos(self, id, file, vol, pitch)
        if (id == "announcer" or id == "announcer1") and type(file) == "string" and file ~= "_STOP" and (vol or 1) > 0 then
            SND.once[#SND.once + 1] = { { file }, math.min(vol or 1, 1), pitch or 1 }
        end
    end
    function __sounds()
        local loops = {}
        for id, s in pairs(SND.loops) do
            local f = sound_files(id)
            if f then loops[#loops + 1] = { id, f, s[1], s[2] } end
        end
        local o = SND.once
        SND.once = {}
        return loops, o
    end
    -- a click is a press followed by a release; returns the switched system's value for display
    function __button(id, down)
        T:ButtonEvent(id, down)
        if not down then return end
        local name = id:gsub("Toggle$", ""):gsub("Set$", "")
        local s = T[name] or T.Systems[name]
        if s and s.Value ~= nil then return tostring(s.Value) end
    end
    -- cab state for save/load: the latched value of every toggle switch, plus the controller and reverser
    function __save_cab(ids)
        local out = {}
        for _, id in ipairs(ids) do
            local name = id:gsub("Toggle$", "")
            local s = T[name]
            if type(s) == "table" and type(s.Value) == "number" then out[name] = s.Value end
        end
        out.__kv = T.KV.ControllerPosition
        out.__rev = T.KV.ReverserPosition
        return out
    end
    function __load_cab(t)
        for name, v in pairs(t) do
            if name:sub(1, 2) ~= "__" then
                local s = T[name]
                if type(s) == "table" and s.TriggerInput and s.Value ~= v then s:TriggerInput("Set", v) end
            end
        end
        if t.__rev then T.KV:TriggerInput("ReverserSet", t.__rev) end
        if t.__kv then T.KV:TriggerInput("ControllerSet", t.__kv) end
    end
    function __press(id)
        T:ButtonEvent(id, true)
        T:ButtonEvent(id, false)
        local name = id:gsub("Toggle$", ""):gsub("Set$", "")
        local s = T[name] or T.Systems[name]
        if s and s.Value ~= nil then return tostring(s.Value) end
    end
"#;

pub struct LoopSound {
    pub id: String,
    pub files: Vec<String>,
    pub volume: f32,
    pub pitch: f32,
}

pub struct OnceSound {
    pub files: Vec<String>,
    pub volume: f32,
    pub pitch: f32,
}

pub struct PropDef {
    pub name: String,
    pub model: String,
    pub pos: [f32; 3],
    pub ang: [f32; 3],
}

/// Speed of the train ahead, m/s (~43 km/h).
const LEAD_SPEED: f32 = 12.0;

pub struct Sim {
    rt: lua::Runtime,
    dynamics: RefCell<Dynamics>,
    track: RefCell<Option<crate::track::Track>>,
    signs: RefCell<Vec<crate::track::Sign>>,
    obstacles: RefCell<Vec<f32>>,
    platforms: RefCell<Vec<crate::station::Platform>>,
    station_names: RefCell<HashMap<u32, String>>,
    /// Tail position (m along the main line) of the moving train ahead, if any.
    lead: RefCell<Vec<f32>>,
    cars: std::cell::Cell<usize>,
    auto_fail: std::cell::Cell<bool>,
    fail_clock: std::cell::Cell<f32>,
    /// Position of the red signal ahead; running past it trips the autostop.
    red_ahead: std::cell::Cell<Option<f32>>,
    moving: std::cell::Cell<bool>,
    /// Last stop on a platform: metres short of the platform end and seconds left to show it.
    stop_report: std::cell::Cell<Option<(f32, f32)>>,
    ars_tripped: std::cell::Cell<bool>,
    /// The driver's trip record (stops, rule violations).
    pub trip: RefCell<crate::trip::Trip>,
    trip_clock: std::cell::Cell<f32>,
}

/// Distance between the centres of coupled 81-717 cars, metres.
const CAR_PITCH: f32 = 19.2;

impl Sim {
    /// Contact-rail voltage seen by both bogeys (0 = dead section).
    pub fn set_rail_voltage(&self, v: f64) {
        let _ = self.rt.lua.globals().set("__rail", v);
    }

    pub fn signs(&self) -> Vec<crate::track::Sign> {
        self.signs.borrow().clone()
    }

    /// Pose in map space of a car standing with its tail at `s` metres along the main line (centre 9 m ahead).
    pub fn standing_pose(&self, s: f32) -> Option<glam::Mat4> {
        let tr = self.track.borrow();
        let p = tr.as_ref()?.main().at(s + 9.0);
        let x = p.dir.normalize_or_zero();
        let y = glam::Vec3::Z.cross(x).normalize_or_zero();
        let z = x.cross(y);
        let o = p.pos + glam::Vec3::Z * crate::app::car_above_rail();
        Some(glam::Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), o.extend(1.0)))
    }

    /// Pose of a car whose centre is `s` metres along the main line; behind the start the line is extended straight back.
    fn car_pose_at(&self, s: f32) -> Option<glam::Mat4> {
        let tr = self.track.borrow();
        let main = tr.as_ref()?.main();
        let p = main.at(s.max(0.0));
        let x = p.dir.normalize_or_zero();
        let y = glam::Vec3::Z.cross(x).normalize_or_zero();
        let z = x.cross(y);
        let o = p.pos + x * s.min(0.0) + glam::Vec3::Z * crate::app::car_above_rail();
        Some(glam::Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), o.extend(1.0)))
    }

    /// Cars coupled behind the driven one (the consist has `cars` in total).
    pub fn set_cars(&self, n: usize) {
        let n = n.clamp(1, 8);
        self.cars.set(n);
        if let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__couple") {
            if let Err(e) = f.call::<()>(n) {
                log::warn!("coupling: {e}");
            }
        }
    }

    pub fn tail_poses(&self) -> Vec<glam::Mat4> {
        let s = self.dynamics.borrow().pos as f32;
        (1..self.cars.get()).filter_map(|k| self.car_pose_at(s - CAR_PITCH * k as f32)).collect()
    }

    /// Everything that is drawn as a separate train: the leads, then our own tail cars.
    pub fn other_poses(&self) -> Vec<glam::Mat4> {
        let mut v = self.lead_poses();
        v.extend(self.tail_poses());
        v
    }

    /// Trains running ahead at LEAD_SPEED; each counts as an occupied block for the signals.
    pub fn set_leads(&self, s: Vec<f32>) {
        *self.lead.borrow_mut() = s;
    }

    pub fn lead_poses(&self) -> Vec<glam::Mat4> {
        self.lead.borrow().iter().filter_map(|&s| self.standing_pose(s)).collect()
    }

    /// Standing trains plus the moving one.
    fn all_obstacles(&self) -> Vec<f32> {
        let mut o = self.obstacles.borrow().clone();
        o.extend(self.lead.borrow().iter());
        o
    }

    pub fn obstacles(&self) -> Vec<f32> {
        self.obstacles.borrow().clone()
    }

    pub fn set_station_names(&self, n: HashMap<u32, String>) {
        *self.station_names.borrow_mut() = n;
    }

    pub fn station_name(&self, id: u32) -> Option<String> {
        self.station_names.borrow().get(&id).cloned()
    }

    pub fn set_platforms(&self, p: Vec<crate::station::Platform>) {
        *self.platforms.borrow_mut() = p;
    }

    /// The platform alongside or next ahead: (station, platform index, metres to its start, side, metres left to its end).
    pub fn platform_ahead(&self) -> Option<(u32, u32, f32, crate::station::Side, f32)> {
        let s = self.dynamics.borrow().pos as f32;
        let ps = self.platforms.borrow();
        let (p, d) = crate::station::ahead(&ps, s)?;
        Some((p.station, p.index, d, p.side.clone(), p.to - s))
    }

    pub fn set_obstacles(&self, o: Vec<f32>) {
        *self.obstacles.borrow_mut() = o;
    }

    /// The code `sign` shows right now (free-line code, or lower when a train blocks the way).
    pub fn aspect(&self, sign: &crate::track::Sign) -> Option<u8> {
        let tr = self.track.borrow();
        let main = tr.as_ref()?.main();
        crate::track::aspect(&self.signs.borrow(), main, sign, &self.all_obstacles())
    }

    pub fn set_signs(&self, s: Vec<crate::track::Sign>) {
        *self.signs.borrow_mut() = s;
    }

    /// (distance m, name, ARS code) of the next signal ahead, if any.
    pub fn next_signal(&self) -> Option<(f32, String, u8)> {
        let tr = self.track.borrow();
        let main = tr.as_ref()?.main();
        let signs = self.signs.borrow();
        let (d, g) = crate::track::next_signal(&signs, main, self.dynamics.borrow().pos as f32)?;
        Some((d, g.name.clone(), crate::track::aspect(&signs, main, g, &self.all_obstacles())?))
    }

    pub fn set_track(&self, t: crate::track::Track) {
        *self.track.borrow_mut() = Some(t);
    }

    /// World pose (metres, Source axes) of the train on the main line: position and unit heading.
    pub fn world_pose(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let tr = self.track.borrow();
        let p = tr.as_ref()?.main();
        let s = p.at(self.dynamics.borrow().pos as f32);
        Some((s.pos, s.dir))
    }

    pub fn set_position(&self, metres: f32) {
        self.dynamics.borrow_mut().pos = metres as f64;
    }

    pub fn set_grade(&self, per_mille: f64) {
        self.dynamics.borrow_mut().grade = per_mille;
    }
}

impl Sim {
    pub fn new(class: &str, lua_root: &str) -> Result<Self> {
        let rt = lua::Runtime::new(Path::new(lua_root))?;
        rt.include("autorun/metrostroi.lua")?;
        let train = rt.spawn_train(class)?;
        rt.lua.globals().set("T", train)?;
        rt.lua.globals().set("__rail", 750.0)?;
        rt.exec(HELPERS, "@sim_helpers")?;
        Ok(Sim { rt, dynamics: RefCell::new(Dynamics::default()), track: RefCell::new(None), signs: RefCell::new(Vec::new()), obstacles: RefCell::new(Vec::new()), platforms: RefCell::new(Vec::new()), station_names: RefCell::new(HashMap::new()), lead: RefCell::new(Vec::new()), cars: std::cell::Cell::new(1), auto_fail: std::cell::Cell::new(false), fail_clock: std::cell::Cell::new(0.0), red_ahead: std::cell::Cell::new(None), moving: std::cell::Cell::new(false), stop_report: std::cell::Cell::new(None), ars_tripped: std::cell::Cell::new(false), trip: RefCell::new(Default::default()), trip_clock: std::cell::Cell::new(0.0) })
    }

    /// The body model of the train (`ENT.Model`, no extension), e.g. `models/metrostroi_train/81-717/81-717_mvm`.
    pub fn body_model(&self) -> Option<String> {
        let m: String = self.rt.lua.load("return C.Model or (ENT and ENT.Model)").eval().ok()?;
        Some(m.trim_end_matches(".mdl").to_string())
    }

    /// Material replacements chosen by the train's skins (texture basename -> material path).
    pub fn skin_map(&self) -> HashMap<String, String> {
        let f: mlua::Function = self.rt.lua.globals().get("__skin_map").unwrap();
        let t: mlua::Table = f.call(()).unwrap();
        t.pairs::<String, String>().filter_map(|r| r.ok()).collect()
    }

    /// Every client prop the cab defines (name, model, Source-space pos and angles).
    pub fn props(&self) -> Vec<PropDef> {
        let f: mlua::Function = self.rt.lua.globals().get("__props").unwrap();
        let list: mlua::Table = f.call(()).unwrap();
        list.sequence_values::<mlua::Table>()
            .filter_map(|r| r.ok())
            .map(|r| PropDef {
                name: r.get(1).unwrap_or_default(),
                model: r.get::<String>(2).unwrap_or_default().trim_end_matches(".mdl").to_string(),
                pos: [r.get(3).unwrap_or(0.0), r.get(4).unwrap_or(0.0), r.get(5).unwrap_or(0.0)],
                ang: [r.get(6).unwrap_or(0.0), r.get(7).unwrap_or(0.0), r.get(8).unwrap_or(0.0)],
            })
            .collect()
    }

    /// Every sound file the cab's Lua can play (for building an asset package).
    pub fn sound_files(&self) -> Vec<String> {
        let code = "local r = {} for _, n in pairs(C.SoundNames or {}) do if type(n) == 'string' then r[#r+1] = n elseif type(n) == 'table' then for _, v in ipairs(n) do if type(v) == 'string' then r[#r+1] = v end end end end return r";
        self.rt.lua.load(code).eval::<mlua::Table>().map(|t| t.sequence_values::<String>().flatten().collect()).unwrap_or_default()
    }

    /// The cab's button panels straight from the running Lua.
    pub fn panels(&self, tips_file: Option<&Path>) -> Option<crate::panels::Panels> {
        let f: mlua::Function = self.rt.lua.globals().get("__panels").ok()?;
        let t: mlua::Table = f.call(()).ok()?;
        let p = crate::panels::Panels::from_lua(&t, tips_file);
        (p.button_count() > 0).then_some(p)
    }

    /// Currently shown props: pose-parameter value (if the cab code set one) and brightness 0..1.
    pub fn state(&self) -> HashMap<String, (Option<f32>, f32)> {
        let f: mlua::Function = self.rt.lua.globals().get("__state").unwrap();
        let t: mlua::Table = f.call(()).unwrap();
        t.pairs::<String, mlua::Table>()
            .filter_map(|r| r.ok())
            .map(|(k, v)| {
                let pose = match v.get::<Value>(1) {
                    Ok(Value::Number(n)) => Some(n as f32),
                    // Lua 5.1 has one number type; mlua hands whole values (0, 1) over as integers
                    Ok(Value::Integer(n)) => Some(n as f32),
                    _ => None,
                };
                (k, (pose, v.get::<f32>(2).unwrap_or(255.0) / 255.0))
            })
            .collect()
    }

    /// Looping sounds (id, files, volume, pitch) and one-shots (files, volume, pitch) raised since the last call.
    pub fn sounds(&self) -> (Vec<LoopSound>, Vec<OnceSound>) {
        let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__sounds") else { return Default::default() };
        let Ok((loops, once)) = f.call::<(mlua::Table, mlua::Table)>(()) else { return Default::default() };
        let files = |t: mlua::Table| t.sequence_values::<String>().filter_map(|r| r.ok()).collect::<Vec<_>>();
        let loops = loops
            .sequence_values::<mlua::Table>()
            .filter_map(|r| r.ok())
            .map(|t| LoopSound { id: t.get(1).unwrap_or_default(), files: files(t.get(2).unwrap()), volume: t.get(3).unwrap_or(0.0), pitch: t.get(4).unwrap_or(1.0) })
            .collect();
        let once = once
            .sequence_values::<mlua::Table>()
            .filter_map(|r| r.ok())
            .map(|t| OnceSound { files: files(t.get(1).unwrap()), volume: t.get(2).unwrap_or(1.0), pitch: t.get(3).unwrap_or(1.0) })
            .collect();
        (loops, once)
    }

    /// Longitudinal speed (km/h, signed along the car) and distance (m).
    pub fn motion(&self) -> (f64, f64) {
        let d = self.dynamics.borrow();
        (d.v * 3.6, d.pos)
    }

    /// Reads the bogey fields the Lua set last tick, integrates the train, writes speed back.
    fn physics(&self, dt: f64) {
        if let Some(tr) = self.track.borrow().as_ref() {
            let pos = self.dynamics.borrow().pos as f32;
            let mut d = self.dynamics.borrow_mut();
            d.grade = tr.main().grade_over(pos, 30.0) as f64;
            d.radius = tr.main().radius_over(pos, 40.0) as f64;
        }
        if let Some((_, name, code)) = self.next_signal() {
            if let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__ars") {
                let _ = f.call::<()>((code as i64, name));
            }
        }
        let g = self.rt.lua.globals();
        let (Ok(fi), Ok(fo)) = (g.get::<mlua::Function>("__phys_in"), g.get::<mlua::Function>("__phys_out")) else { return };
        let Ok(t) = fi.call::<mlua::Table>(()) else { return };
        let n = t.raw_len() as i64;
        let read = |i: i64| -> BogeyIn {
            let r: mlua::Table = t.get(i).unwrap();
            let n = |k: i64| r.get::<f64>(k).unwrap_or(0.0);
            BogeyIn { motor_force: n(1), motor_power: n(2), reversed: n(3) > 0.5, pneumatic_force: n(4), brake_cylinder: n(5), parking: n(6), pneumatic_pow: n(7) }
        };
        let ins: Vec<BogeyIn> = (1..=n).map(read).collect();
        let out = self.dynamics.borrow_mut().step(dt, &ins);
        let flat: Vec<f64> = out.iter().flat_map(|o| [o.speed, o.speed_sign, o.acceleration]).collect();
        let _ = fo.call::<()>(flat);
    }

    /// Start the scripted cab start-up (battery, brake valve, reverser, overload reset); KV ends in 0.
    pub fn quick_start(&self) {
        let f: mlua::Function = self.rt.lua.globals().get("__quickstart").unwrap();
        let _ = f.call::<()>(());
    }

    fn quick_start_active(&self) -> bool {
        self.rt.lua.globals().get::<mlua::Function>("__qs_active").and_then(|f| f.call::<bool>(())).unwrap_or(false)
    }

    pub fn tick(&self, dt: f32) {
        if self.auto_fail.get() {
            let t = self.fail_clock.get() + dt;
            if t >= 1.0 {
                self.fail_clock.set(0.0);
                if let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__fail_age") {
                    let _ = f.call::<()>(t as f64);
                }
            } else {
                self.fail_clock.set(t);
            }
        }
        for s in self.lead.borrow_mut().iter_mut() {
            *s += LEAD_SPEED * dt;
        }
        // the start-up script runs 6x faster than real time
        for _ in 0..if self.quick_start_active() { 6 } else { 1 } {
            self.tick_once(dt);
        }
    }

    /// ARS stand-in: the 717's own BARS needs a long chain of cab switches, so overspeed against the ALS code is
    /// enforced here by tripping the autostop (emergency valve) like the real ARS would.
    fn enforce_ars(&self) {
        let next = self.next_signal();
        let pos = self.dynamics.borrow().pos as f32;
        if let Some(red) = self.red_ahead.get() {
            if pos > red + 3.0 {
                self.red_ahead.set(None);
                let _ = self.rt.lua.load("T.Pneumatic:TriggerInput('Autostop', 0)").exec();
                if !self.ars_tripped.replace(true) {
                    log::warn!("ARS: passed a red signal: autostop");
                }
            }
        }
        if let Some((d, _, 0)) = &next {
            self.red_ahead.set(Some(pos + d));
        } else if next.as_ref().is_some_and(|n| n.0 > 30.0) {
            self.red_ahead.set(None);
        }
        let Some((_, name, code)) = next else { return };
        let limit = match code {
            8 => 80.0,
            7 => 70.0,
            6 => 60.0,
            4 => 40.0,
            _ => return,
        };
        if self.motion().0.abs() > limit + 3.0 {
            let _ = self.rt.lua.load("T.Pneumatic:TriggerInput('Autostop', 0)").exec();
            if !self.ars_tripped.replace(true) {
                log::warn!("ARS: {:.0} km/h over code {code} ({limit} km/h) at {name}: autostop", self.motion().0.abs());
            }
        }
    }

    /// Speed limit (km/h) of the ALS code currently ahead, if it has one.
    pub fn ars_limit(&self) -> Option<f32> {
        match self.next_signal()?.2 {
            8 => Some(80.0),
            7 => Some(70.0),
            6 => Some(60.0),
            4 => Some(40.0),
            _ => None,
        }
    }

    pub fn ars_tripped(&self) -> bool {
        self.ars_tripped.get()
    }

    /// Switch ageing of relays and contactors (random failures over time) on or off.
    pub fn set_auto_failures(&self, on: bool) {
        self.auto_fail.set(on);
        let _ = self.rt.lua.load(&format!("FAIL_AUTO = {on}")).exec();
    }

    pub fn auto_failures(&self) -> bool {
        self.auto_fail.get()
    }

    /// A random failure like `metrostroi_fail`: the description of what broke.
    pub fn trigger_failure(&self) -> Option<String> {
        let f: mlua::Function = self.rt.lua.globals().get("__fail_now").ok()?;
        f.call::<Option<String>>(()).ok().flatten().map(|s| s.replace("[!FAIL!]", "").trim().to_string())
    }

    /// Like `metrostroi_fail_reset`.
    pub fn reset_failures(&self) {
        if let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__fail_reset") {
            let _ = f.call::<()>(());
        }
    }

    /// Descriptions of the failures that are active now.
    pub fn failures(&self) -> Vec<String> {
        let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__fail_list") else { return Vec::new() };
        f.call::<mlua::Table>(()).map(|t| t.sequence_values::<String>().flatten().collect()).unwrap_or_default()
    }

    /// Release the autostop (what the driver does with the UAVA key).
    pub fn reset_autostop(&self) {
        self.ars_tripped.set(false);
        let _ = self.rt.lua.load("T.Pneumatic.EmergencyValve = false T.UAVAC:TriggerInput('Set', 1)").exec();
    }

    /// Which sides have a passenger door open: (left, right).
    pub fn doors_open(&self) -> (bool, bool) {
        let r = self.rt.lua.load(
            "local l, r = 0, 0 for i = 1, 4 do l = math.max(l, T:GetPackedRatio('DoorL' .. i) or 0) r = math.max(r, T:GetPackedRatio('DoorR' .. i) or 0) end return l > 0.05, r > 0.05",
        );
        r.eval::<(bool, bool)>().unwrap_or((false, false))
    }

    pub fn stop_report(&self) -> Option<(f32, f32)> {
        self.stop_report.get().filter(|r| r.1 > 0.0)
    }

    /// Notes when the train comes to rest alongside a platform and how far from its end it stopped.
    fn track_stops(&self, dt: f32) {
        let v = self.dynamics.borrow().v.abs();
        if let Some((o, t)) = self.stop_report.get() {
            self.stop_report.set(Some((o, t - dt)));
        }
        if v > 1.0 {
            self.moving.set(true);
        } else if v < 0.05 && self.moving.replace(false) {
            // a stop counts at the platform whose end is nearest, also a few metres past it (an overrun is scored too)
            let pos = self.dynamics.borrow().pos as f32;
            let hit = self.platform_at(pos).map(|p| (p.station, p.to - pos));
            if let Some((station, left)) = hit {
                self.stop_report.set(Some((left, 10.0)));
                let name = self.station_name(station).unwrap_or_else(|| "станция".into());
                self.trip.borrow_mut().stopped(name, left);
            }
        }
    }

    /// The platform a train standing at `pos` belongs to: the one whose end is nearest within its length and a short overrun.
    fn platform_at(&self, pos: f32) -> Option<crate::station::Platform> {
        let ps = self.platforms.borrow();
        ps.iter().filter(|p| pos >= p.from - 8.0 && pos <= p.to + 25.0).min_by(|a, b| (a.to - pos).abs().total_cmp(&(b.to - pos).abs())).cloned()
    }

    /// Samples the facts the trip record needs five times a second.
    fn track_trip(&self, dt: f32) {
        let t = self.trip_clock.get() + dt;
        if t < 0.2 {
            self.trip_clock.set(t);
            return;
        }
        self.trip_clock.set(0.0);
        let (dl, dr) = self.doors_open();
        let pos = self.dynamics.borrow().pos as f32;
        let wrong = match self.platform_at(pos) {
            Some(p) => {
                let left = p.side == crate::station::Side::Left;
                (dl && !left) || (dr && left)
            }
            None => true,
        };
        let kmh = self.dynamics.borrow().v.abs() as f32 * 3.6;
        self.trip.borrow_mut().observe(t, kmh, dl || dr, wrong, self.ars_tripped.get());
    }

    pub fn trip_lines(&self) -> Vec<String> {
        self.trip.borrow().lines()
    }

    fn tick_once(&self, dt: f32) {
        self.enforce_ars();
        self.physics(dt as f64);
        self.track_stops(dt);
        self.track_trip(dt);
        let f: mlua::Function = self.rt.lua.globals().get("__tick").unwrap();
        if let Ok(Some(err)) = f.call::<Option<String>>(dt as f64) {
            log::warn!("train think: {err}");
        }
    }

    pub fn hidden_buttons(&self) -> HashSet<String> {
        let f: mlua::Function = self.rt.lua.globals().get("__hidden_buttons").unwrap();
        let t: mlua::Table = f.call(()).unwrap();
        t.sequence_values::<String>().filter_map(|r| r.ok()).collect()
    }

    pub fn key(&self, name: &str, down: bool) {
        let f: mlua::Function = self.rt.lua.globals().get("__key").unwrap();
        if let Ok(Some(err)) = f.call::<Option<String>>((name.to_string(), down)) {
            log::warn!("key {name}: {err}");
        }
    }

    /// Mouse down (`down`) or up on a cab button; buttons like the return/vigilance keys need to be held.
    pub fn button(&self, id: &str, down: bool) -> Option<String> {
        let f: mlua::Function = self.rt.lua.globals().get("__button").ok()?;
        f.call::<Option<String>>((id.to_string(), down)).ok().flatten()
    }

    /// The latched state of the cab's toggle switches (ids from the button panels), as JSON.
    pub fn save_cab(&self, ids: &[String]) -> serde_json::Value {
        let Ok(f) = self.rt.lua.globals().get::<mlua::Function>("__save_cab") else { return serde_json::Value::Null };
        let Ok(t) = f.call::<mlua::Table>(ids.to_vec()) else { return serde_json::Value::Null };
        let map: serde_json::Map<String, serde_json::Value> =
            t.pairs::<String, f64>().flatten().map(|(k, v)| (k, serde_json::json!(v))).collect();
        serde_json::Value::Object(map)
    }

    pub fn load_cab(&self, state: &serde_json::Value) {
        let (Some(map), Ok(f)) = (state.as_object(), self.rt.lua.globals().get::<mlua::Function>("__load_cab")) else { return };
        let Ok(t) = self.rt.lua.create_table() else { return };
        for (k, v) in map {
            if let Some(n) = v.as_f64() {
                let _ = t.set(k.as_str(), n);
            }
        }
        let _ = f.call::<()>(t);
    }

    pub fn press(&self, id: &str) -> Option<String> {
        let f: mlua::Function = self.rt.lua.globals().get("__press").ok()?;
        f.call::<Option<String>>(id.to_string()).ok().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sim: &Sim, seconds: f32) {
        for _ in 0..(seconds * 60.0) as u32 {
            sim.tick(1.0 / 60.0);
        }
    }

    fn lua_bool(sim: &Sim, expr: &str) -> bool {
        sim.rt.lua.load(format!("return {expr}")).eval().unwrap()
    }

    /// The real start-up from the cab, then KV into X3: the car accelerates, then T2 and the brake stop it.
    #[test]
    fn drive_and_stop() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        // the 717 powers up with its breakers closed; switch on only what is still off
        for name in ["VB", "VAH", "VAD"] {
            if !lua_bool(&sim, &format!("T.{name}.Value > 0.5")) {
                sim.press(&format!("{name}Toggle"));
            }
        }
        for id in ["DriverValveBLDisconnectToggle", "DriverValveTLDisconnectToggle", "DriverValveDisconnectToggle"] {
            sim.press(id);
        }
        for k in ["PAD_ENTER", "PAD_PLUS", "PAD_1"] {
            sim.key(k, true);
            sim.tick(1.0 / 60.0);
            sim.key(k, false);
        }
        run(&sim, 10.0);
        // RVT is energised in the T positions, so a start goes T1 -> 0 -> X; the overload relay
        // is reset with the return button held while KV sits in 0
        sim.key("5", true);
        run(&sim, 3.0);
        sim.key("4", true);
        sim.rt.lua.load("T:ButtonEvent('VozvratRPSet', true)").exec().unwrap();
        run(&sim, 0.5);
        sim.rt.lua.load("T:ButtonEvent('VozvratRPSet', false)").exec().unwrap();
        sim.key("5", true);
        run(&sim, 2.0);
        sim.key("4", true);
        run(&sim, 0.1);
        sim.key("3", true);
        run(&sim, 10.0);
        let (kmh, pos) = sim.motion();
        println!("10 s in X3: {kmh:.1} km/h, {pos:.0} m");
        let (loops, _) = sim.sounds();
        let rolling: Vec<_> = loops.iter().filter(|l| l.id.starts_with("rolling") && l.volume > 0.01).map(|l| &l.id).collect();
        println!("rolling loops: {rolling:?}");
        assert!(!rolling.is_empty(), "no rolling noise at speed");
        assert!(kmh > 25.0 && kmh < 80.0, "speed after 10 s in X3: {kmh}");
        let accel = {
            let a = sim.motion().0;
            run(&sim, 1.0);
            (sim.motion().0 - a) / 3.6
        };
        println!("acceleration near {kmh:.0} km/h: {accel:.2} m/s^2");

        sim.key("7", true); // T2
        run(&sim, 40.0);
        let (kmh2, _) = sim.motion();
        println!("40 s after T2: {kmh2:.1} km/h");
        assert!(sim.world_pose().is_none());
        assert!(kmh2 < kmh * 0.5, "T2 did not slow the train: {kmh} -> {kmh2}");
    }

    /// The performance envelope of one 81-717 on level track: acceleration, top speed, coasting and braking.
    #[test]
    fn performance_envelope() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        sim.key("3", true);
        let mut speeds = vec![];
        for _ in 0..24 {
            run(&sim, 5.0);
            speeds.push(sim.motion().0);
        }
        let accel0 = speeds[0] / 3.6 / 5.0;
        let top = speeds.iter().cloned().fold(0.0, f64::max);
        println!("speed every 5 s: {speeds:.0?}");
        println!("mean acceleration over the first 5 s: {accel0:.2} m/s^2, top speed {top:.0} km/h");
        // coasting
        let kvp = |sim: &Sim, tag: &str| {
            let v: String = sim.rt.lua.load("return tostring(T.KV.ControllerPosition)").eval().unwrap();
            println!("KV {tag}: {v}");
        };
        kvp(&sim, "before release");
        sim.key("3", false);
        kvp(&sim, "after 3 up");
        sim.key("4", true);
        kvp(&sim, "after 4 down");
        run(&sim, 1.0);
        sim.key("4", false);
        let kv: String = sim.rt.lua.load("return tostring(T.KV.ControllerPosition) .. ' en=' .. tostring(T.KV.Enabled) .. ' rev=' .. tostring(T.KV.ReverserPosition)").eval().unwrap();
        println!("KV after key 4 release: {kv}");
        let kb: String = sim.rt.lua.load("local t = {} for k, v in pairs(T.KeyBuffer or {}) do t[#t + 1] = tostring(k) .. '=' .. tostring(v) end return table.concat(t, ',') .. ' | KEY_4=' .. tostring(KEY_4) .. ' map=' .. tostring(T.KeyMap[KEY_4]) .. ' mods=' .. tostring(T.KeyMods and T.KeyMods[KEY_4] and next(T.KeyMods[KEY_4]))").eval().unwrap();
        println!("KeyBuffer: {kb}");
        let e: String = sim.rt.lua.load("local ok, err = pcall(T.OnKeyEvent, T, KEY_4, true, nil) return tostring(ok) .. ' ' .. tostring(err) .. ' KV=' .. T.KV.ControllerPosition").eval().unwrap();
        println!("OnKeyEvent: {e}");
        sim.rt.lua.load("T:ButtonEvent('KVSet0', true)").exec().unwrap();
        run(&sim, 1.0);
        let kv: String = sim.rt.lua.load("return tostring(T.KV.ControllerPosition)").eval().unwrap();
        println!("KV after ButtonEvent: {kv}");
        let v0 = sim.motion().0;
        let dbg: String = sim.rt.lua.load("return string.format('KV=%s MP=%.2f MF=%.0f I=%.0f', tostring(T.KV.ControllerPosition) .. '/' .. tostring(T.KV.RealControllerPosition), T.FrontBogey.MotorPower or -9, T.FrontBogey.MotorForce or -9, (T.Electric and T.Electric.Itotal) or -9)").eval().unwrap();
        println!("after key 4: {dbg}");
        for _ in 0..4 {
            run(&sim, 5.0);
            let d: String = sim.rt.lua.load("return string.format('MP=%.2f MF=%.0f I=%.0f v=%.1f', T.FrontBogey.MotorPower or -9, T.FrontBogey.MotorForce or -9, (T.Electric and T.Electric.Itotal) or -9, T.FrontBogey.Speed or -1)").eval().unwrap();
            println!("  coast: {d}");
        }
        let coast = (v0 - sim.motion().0) / 3.6 / 20.0;
        println!("coasting from {v0:.0} km/h: {coast:.3} m/s^2");
        // service brake
        let v1 = sim.motion().0;
        sim.key("7", true);
        run(&sim, 3.0);
        let d: String = sim.rt.lua.load("return string.format('BC=%.2f TL=%.2f BL=%.2f mp=%.2f mf=%.0f I=%.0f', T.Pneumatic.BrakeCylinderPressure, T.Pneumatic.TrainLinePressure, T.Pneumatic.BrakeLinePressure, T.FrontBogey.MotorPower or -9, T.FrontBogey.MotorForce or -9, T.Electric.Itotal or -9)").eval().unwrap();
        println!("  T2: {d}");
        let (v2, p2) = (sim.motion().0, sim.motion().1);
        run(&sim, 5.0);
        let brake = (v2 - sim.motion().0) / 3.6 / 5.0;
        println!("T2 from {v1:.0} km/h: {brake:.2} m/s^2 (pos {p2:.0})");
        // emergency brake from what is left of the speed (the valve stays in its service position otherwise)
        sim.key("3", true);
        run(&sim, 15.0);
        sim.rt.lua.load("T:ButtonEvent('EmergencyBrake', true)").exec().unwrap();
        run(&sim, 2.0);
        let (ve, pe) = (sim.motion().0, sim.motion().1);
        for _ in 0..4 {
            run(&sim, 1.0);
            let d: String = sim.rt.lua.load("return string.format('BC=%.2f TL=%.2f BL=%.2f v=%.1f BrakeForce=%.0f', T.Pneumatic.BrakeCylinderPressure, T.Pneumatic.TrainLinePressure, T.Pneumatic.BrakeLinePressure, T.FrontBogey.Speed, T.FrontBogey.PneumaticBrakeForce or -1)").eval().unwrap();
            println!("  EB: {d}");
        }
        let eb = (ve - sim.motion().0) / 3.6 / 4.0;
        println!("emergency brake from {ve:.0} km/h: {eb:.2} m/s^2 (pos {pe:.0})");
        assert!(eb > 0.9 && eb < 1.6, "emergency braking {eb}");
        assert!(accel0 > 0.7 && accel0 < 1.5, "acceleration {accel0}");
        // the original compensates Source's friction (`compensateF`), so the limit comes from the motor curve, not drag
        assert!(top > 80.0 && top < 110.0, "top speed {top}");
        assert!(coast > 0.0 && coast < 0.15, "coasting deceleration {coast}");
        assert!(brake > 0.6 && brake < 1.6, "service braking {brake}");
    }

    /// Station to station on a generated Moscow line, driven by a simple automatic driver: the stop is recorded.
    #[test]
    fn a_driver_can_stop_at_the_next_station() {
        let dir = Path::new("E:/metrostroi2pz/game/content/maps/moscow/305810");
        if !Path::new(LUA_ROOT).exists() || !dir.join("stations.json").exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        let t = crate::track::Track::load(&dir.join("track.lua")).unwrap();
        let (plats, names) = crate::station::from_json(&dir.join("stations.json")).unwrap();
        sim.set_platforms(plats.clone());
        sim.set_station_names(names);
        sim.set_signs(Vec::new());
        sim.set_track(t);
        sim.set_position(plats[0].to - 10.0);
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        let target = plats[1].to - 4.0; // stop four metres short of the end of the second platform
        sim.key("3", true);
        let mut state = "power";
        for step in 0..(60 * 400) {
            sim.tick(1.0 / 60.0);
            if step % 6 != 0 {
                continue;
            }
            let (kmh, pos) = sim.motion();
            let left = target as f64 - pos;
            let v = kmh / 3.6;
            match state {
                "power" if kmh > 55.0 => {
                    sim.key("3", false);
                    sim.key("4", true);
                    sim.key("4", false);
                    state = "coast";
                }
                "coast" if left < v * v / (2.0 * 1.1) + 8.0 => {
                    sim.key("7", true);
                    state = "brake";
                }
                "brake" if kmh < 12.0 => {
                    sim.rt.lua.load("T.Pneumatic:TriggerInput('BrakeSet', 5)").exec().unwrap();
                    state = "hold";
                }
                _ => {}
            }
            if state == "hold" && kmh < 0.1 {
                break;
            }
        }
        let trip = sim.trip.borrow();
        println!("{:?} -> {:?} after {:.0} m", state, trip.stops, sim.motion().1);
        assert_eq!(trip.stops.len(), 1, "no stop recorded: {:?}", trip.stops);
        assert!(trip.stops[0].short > -3.0 && trip.stops[0].short < 12.0, "stopped {} m short of the end", trip.stops[0].short);
        assert_eq!(trip.autostops, 0);
    }

    /// With a track and signs loaded, the ALS coil receives the next signal's ARS code.
    #[test]
    fn als_coil_gets_code_from_next_signal() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\crossline_c4");
        if !Path::new(LUA_ROOT).exists() || !dir.join("signs.lua").exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        let t = crate::track::Track::load(&dir.join("track.lua")).unwrap();
        sim.set_signs(crate::track::load_signs(&dir.join("signs.lua"), t.main()).unwrap());
        sim.set_track(t);
        run(&sim, 3.0);
        // the cab's ALS switch normally enables the coil; drive it directly here
        sim.rt.lua.load("T.ALSCoil.Enabled = 1 T.ALSCoil.Timer = -10 T.ALSCoil:Think(0.1)").exec().unwrap();
        let (d, name, code) = sim.next_signal().expect("a signal ahead");
        println!("next signal {name} in {d:.0} m, code {code}");
        let f: f64 = sim.rt.lua.load("return T.ALSCoil.F1+T.ALSCoil.F2+T.ALSCoil.F3+T.ALSCoil.F4+T.ALSCoil.F5").eval().unwrap();
        assert!(f > 0.5 || code == 0 || code == 1, "ALS coil got no frequency (code {code})");
    }

    /// A 60 km/h code ahead: the train accelerates to it, the ARS stand-in trips the autostop, the train stops.
    #[test]
    fn ars_stops_an_overspeeding_train() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\crossline_c4");
        if !Path::new(LUA_ROOT).exists() || !dir.join("signs.lua").exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        let t = crate::track::Track::load(&dir.join("track.lua")).unwrap();
        sim.set_signs(crate::track::load_signs(&dir.join("signs.lua"), t.main()).unwrap());
        sim.set_track(t);
        sim.set_position(300.0);
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        sim.key("3", true);
        let mut max = 0.0f64;
        for _ in 0..90 {
            run(&sim, 1.0);
            max = max.max(sim.motion().0);
        }
        let end = sim.motion().0;
        println!("max {max:.0} km/h, after 90 s {end:.0} km/h, tripped {}", sim.ars_tripped.get());
        assert!(sim.ars_tripped.get(), "ARS never tripped (max {max})");
        assert!(max < 90.0 && end < 5.0, "max {max}, end {end}");
    }

    /// Cold and dark: nothing but the cab's own controls (battery, valves, wrench, reverser, doors, KV).
    #[test]
    fn cold_and_dark_manual_start() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        let status = |sim: &Sim, tag: &str| {
            let s: String = sim.rt.lua.load("return string.format('BAT=%.0f BC=%.2f TL=%.1f KD=%s VUD=%s DoorsOpen=%s RV2=%s R1_5=%s RPv=%s W1=%s W6=%s LK1=%s VB=%s VAH=%s VAD=%s', T.Battery.Voltage, T.Pneumatic.BrakeCylinderPressure, T.Pneumatic.TrainLinePressure, tostring(T.KD.Value), tostring(T.VUD1 and T.VUD1.Value), tostring(T.DoorsOpen), tostring(T.RV2.Value), tostring(T.R1_5.Value), tostring(T.RPvozvrat.Value), tostring(T:ReadTrainWire(1)), tostring(T:ReadTrainWire(6)), tostring(T.LK1.Value), tostring(T.VB.Value), tostring(T.VAH.Value), tostring(T.VAD.Value))").eval().unwrap();
            println!("{tag}: {s} v={:.1}", sim.motion().0);
        };
        status(&sim, "dark");
        let click = |id: &str| {
            sim.button(id, true);
            run(&sim, 0.05);
            sim.button(id, false);
        };
        let hold = |id: &str, secs: f32| {
            sim.button(id, true);
            run(&sim, secs);
            sim.button(id, false);
        };
        click("VBToggle");
        run(&sim, 3.0);
        status(&sim, "battery");
        for id in ["DriverValveBLDisconnectToggle", "DriverValveTLDisconnectToggle", "DriverValveDisconnectToggle"] {
            click(id);
        }
        // wrench in, doors closed with the reverser still neutral (that is when KD latches), then direction
        sim.key("PAD_ENTER", true);
        run(&sim, 0.05);
        sim.key("PAD_ENTER", false);
        // a single car has no second cab to latch KD, so the door-circuit bypass VAD is used (as on a one-car run)
        click("VUD1Toggle");
        click("VADToggle");
        run(&sim, 3.0);
        for k in ["PAD_PLUS", "PAD_1"] {
            sim.key(k, true);
            run(&sim, 0.05);
            sim.key(k, false);
        }
        run(&sim, 8.0);
        status(&sim, "valves");
        status(&sim, "doors closed");
        let d: String = sim.rt.lua.load("local S=T.Schemes return string.format('BD=%s RD=%s D4=%s w-28=%s w28=%s w15=%s D8-15A=%s D4-15=%s ReverserPos=%s', tostring(T.BD.Value), tostring(T.RD.Value), tostring(S.D4), tostring(T:ReadTrainWire(-28)), tostring(T:ReadTrainWire(28)), tostring(T:ReadTrainWire(15)), tostring(T.KV['D8-15A']), tostring(T.KV['D4-15']), tostring(T.KV.ReverserPosition))").eval().unwrap();
        println!("doors: {d}");
        sim.key("5", true);
        run(&sim, 3.0);
        sim.key("4", true);
        hold("VozvratRPSet", 0.6);
        sim.key("5", true);
        run(&sim, 2.0);
        status(&sim, "T1");
        sim.key("4", true);
        run(&sim, 0.1);
        sim.key("3", true);
        run(&sim, 12.0);
        status(&sim, "X3");
        assert!(sim.motion().0 > 15.0, "did not move");
    }

    #[test]
    fn door_probe() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        sim.key("4", true);
        run(&sim, 2.0);
        let show = |tag: &str| {
            let s: String = sim.rt.lua.load("return string.format('DoorL1=%.2f DoorR1=%.2f LeftOpen=%s KDL=%s KDLK=%s DoorSelect=%s VDL=%s W31=%s', T:GetPackedRatio('DoorL1'), T:GetPackedRatio('DoorR1'), tostring(T.LeftDoorsOpen), tostring(T.KDL.Value), tostring(T.KDLK.Value), tostring(T.DoorSelect.Value), tostring(T.VDL.Value), tostring(T:ReadTrainWire(31)))").eval().unwrap();
            let st = sim.state();
            println!("{tag}: {s} pose door0x1={:?}", st.get("door0x1").map(|v| v.0));
        };
        show("closed");
        sim.button("KDLSet", true);
        run(&sim, 3.0);
        show("KDL held");
        println!("doors_open: {:?}", sim.doors_open());
        assert!(sim.doors_open().0 || sim.doors_open().1, "no door reported open while KDL is held");
        sim.button("KDLSet", false);
        run(&sim, 2.0);
        show("KDL released");
        sim.button("VUD1Toggle", true);
        sim.button("VUD1Toggle", false);
        run(&sim, 4.0);
        show("VUD1 toggled");
        sim.button("VUD1Set", true);
        run(&sim, 3.0);
        sim.button("VUD1Set", false);
        show("VUD1Set held");
    }

    #[test]
    fn toggle_probe() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        let before = sim.state();
        let keys: Vec<_> = before.keys().filter(|k| k.contains("R_Radio")).cloned().collect();
        println!("keys {keys:?}");
        sim.press("R_RadioToggle");
        run(&sim, 1.0);
        let after = sim.state();
        for k in &keys {
            println!("{k}: {:?} -> {:?}", before.get(k), after.get(k));
        }
        let changed = before.iter().filter(|(k, v)| after.get(*k).is_some_and(|a| a.0 != v.0 || a.1 != v.1)).count();
        println!("{changed} props changed of {}", before.len());
        let dbg: String = sim.rt.lua.load("local n='R_RadioToggle' local a=C.Anims[n] local e=C.ClientEnts[n] return string.format('anim=%s val=%s value=%s hidden=%s hanim=%s ent=%s pose=%s', tostring(a), a and tostring(a.val), a and tostring(a.value), tostring(C.Hidden[n]), tostring(C.Hidden.anim[n]), tostring(e), e and tostring(e.__pose and e.__pose.position))").eval().unwrap();
        println!("{dbg}");
    }

    #[test]
    fn lead_train_moves_and_counts_as_obstacle() {
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        sim.set_leads(vec![100.0, 300.0]);
        sim.tick(1.0);
        assert!((sim.lead.borrow()[0] - 100.0 - LEAD_SPEED).abs() < 1e-3);
        assert_eq!(sim.all_obstacles().len(), 2);
    }

    #[test]
    fn coupled_cars_trail_the_head_car_by_one_car_length() {
        let f = Path::new("E:/metrostroi2pz/game/content/maps/crossline_c4/track.lua");
        if !f.exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        sim.set_track(crate::track::Track::load(f).unwrap());
        sim.set_cars(4);
        sim.set_position(500.0);
        let tail = sim.tail_poses();
        assert_eq!(tail.len(), 3);
        let head = sim.car_pose_at(500.0).unwrap().w_axis.truncate();
        let d1 = (tail[0].w_axis.truncate() - head).length();
        assert!((d1 - CAR_PITCH).abs() < 0.5, "first tail car {d1} m behind");
        // at the very start the tail runs back along the line instead of piling up
        sim.set_position(0.0);
        let t = sim.tail_poses();
        assert!((t[2].w_axis.truncate() - t[1].w_axis.truncate()).length() > 15.0);
    }

    /// Real coupling: the trailer's train wires follow the head car's cab (Metrostroi's own WagonList code).
    #[test]
    fn trailer_follows_the_head_car_train_wires() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        sim.set_cars(2);
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        sim.key("3", true);
        run(&sim, 3.0);
        let r: String = sim.rt.lua.load("return string.format('wagons=%d head W1=%s W6=%s trailer W1=%s W6=%s', #T.WagonList, tostring(T:ReadTrainWire(1)), tostring(T:ReadTrainWire(6)), tostring(__cars[2]:ReadTrainWire(1)), tostring(__cars[2]:ReadTrainWire(6)))").eval().unwrap();
        println!("{r}");
        let same: bool = sim.rt.lua.load("return __cars[2]:ReadTrainWire(1) == T:ReadTrainWire(1) and __cars[2]:ReadTrainWire(6) == T:ReadTrainWire(6)").eval().unwrap();
        assert!(same && r.starts_with("wagons=2"), "{r}");
        // the trailer's bogeys read the same speed as the head car's (door interlocks and the like depend on it)
        sim.set_position(0.0);
        sim.rt.lua.load("__cars[2].FrontBogey.Speed = -1").exec().unwrap();
        sim.tick(1.0 / 60.0);
        let ok: bool = sim.rt.lua.load("return __cars[2].FrontBogey.Speed == T.FrontBogey.Speed").eval().unwrap();
        assert!(ok, "trailer bogey speed not fed by the dynamics");
    }

    /// A drive over the first section of a Moscow line (with its dip): the train leaves the platform and reaches speed.
    #[test]
    fn train_drives_a_generated_moscow_line() {
        let f = Path::new("E:/metrostroi2pz/game/content/maps/moscow/305810/track.lua");
        if !Path::new(LUA_ROOT).exists() || !f.exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        sim.set_track(crate::track::Track::load(f).unwrap());
        sim.set_position(5.0);
        run(&sim, 2.0);
        sim.quick_start();
        while sim.quick_start_active() {
            sim.tick(1.0 / 60.0);
        }
        sim.key("3", true);
        let mut max = 0.0f64;
        for _ in 0..40 {
            run(&sim, 1.0);
            max = max.max(sim.motion().0);
        }
        let (kmh, pos) = sim.motion();
        println!("Moscow: {kmh:.0} km/h (max {max:.0}) after {pos:.0} m");
        assert!(max > 30.0 && pos > 200.0, "max {max}, pos {pos}");
    }

    #[test]
    fn announcer_messages_reach_the_audio_queue() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 1.0);
        let r: String = sim
            .rt
            .lua
            .load("local A = T.Announcer if not A then return 'no announcer' end local n = 0 for k,v in pairs(Metrostroi[A.AnnTable or ''] and Metrostroi[A.AnnTable][1] or {}) do n = n + 1 end return tostring(A.AnnTable)..' '..n")
            .eval()
            .unwrap();
        println!("announcer: {r}");
        sim.rt.lua.load("T.Announcer:Queue{'buzz_start'} T.Announcer:Think() T.Announcer:Think()").exec().unwrap();
        run(&sim, 1.0);
        let (_, once) = sim.sounds();
        println!("once sounds: {}", once.len());
    }

    /// Saving the cab and loading it again brings the switches back.
    #[test]
    fn cab_state_round_trips() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 3.0);
        let ids: Vec<String> = ["VBToggle", "VAHToggle", "VADToggle", "A1Toggle"].map(String::from).to_vec();
        let val = |n: &str| -> f64 { sim.rt.lua.load(format!("return T.{n}.Value")).eval().unwrap() };
        sim.press("VBToggle"); // VB is off after a fresh spawn: switch it on, then save
        run(&sim, 0.5);
        let saved = sim.save_cab(&ids);
        let (vb, vad) = (val("VB"), val("VAD"));
        sim.press("VBToggle");
        sim.press("VADToggle");
        run(&sim, 0.5);
        assert!(val("VB") != vb || val("VAD") != vad, "presses changed nothing");
        sim.load_cab(&saved);
        run(&sim, 0.5);
        assert_eq!((val("VB"), val("VAD")), (vb, vad), "state not restored: {saved}");
    }

    #[test]
    fn pressing_a_switch_queues_a_click_sound() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 3.0);
        let _ = sim.sounds();
        for id in ["VBToggle", "A1Toggle", "VUToggle"] {
            sim.press(id);
            run(&sim, 0.3);
            let (_, once) = sim.sounds();
            println!("{id}: {} one-shot sounds {:?}", once.len(), once.iter().map(|o| o.files.first().cloned()).collect::<Vec<_>>());
        }
    }

    #[test]
    fn cab_doors_swing() {
        if !Path::new(LUA_ROOT).exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        run(&sim, 2.0);
        let before = sim.state();
        for id in ["CabinDoor", "RearDoor", "PassengerDoor"] {
            println!("press {id}: {:?}", sim.press(id));
        }
        run(&sim, 2.0);
        let after = sim.state();
        for k in ["door1", "door2", "door3"] {
            println!("{k}: {:?} -> {:?}", before.get(k), after.get(k));
        }
    }

    #[test]
    fn button_tags_sit_on_their_models() {
        let controls = Path::new("E:/metrostroi2pz/game/content/trains/81-717_mvm/controls.json");
        if !Path::new(LUA_ROOT).exists() || !controls.exists() {
            return;
        }
        let sim = Sim::new("gmod_subway_81-717_mvm", LUA_ROOT).unwrap();
        let panels = crate::panels::Panels::load(controls).unwrap();
        let props = sim.props();
        for id in ["R_RadioToggle", "VBToggle", "R_UNchToggle", "KVSet0", "VozvratRPSet", "A1Toggle"] {
            let Some(pr) = props.iter().find(|p| p.name == id) else { println!("{id}: no prop"); continue };
            let model = glam::Vec3::from(pr.pos) * crate::renderer::SCALE;
            let (tag, n) = panels.button_pos(id).unwrap();
            let d = tag - model;
            println!("{id}: {:?}", panels.debug_coords(id, model));
            println!("{id}: |tag-model| {:.3} m, along normal {:.3} m, across {:.3} m", d.length(), d.dot(n), (d - n * d.dot(n)).length());
        }
    }
}
