-- Minimal Garry's Mod runtime for running Metrostroi's own Lua (systems, entity code) outside the engine.
SERVER = true
CLIENT = false
GAME_ROOT = GAME_ROOT or "."

local noop = function() end

---------------------------------------------------------------- globals
function AddCSLuaFile() end
function ErrorNoHalt(...) io.stderr:write(table.concat({...}, "\t"), "\n") end
function Error(...) error(table.concat({...}, "\t"), 2) end
function MsgN(...) print(...) end
function Msg(...) io.write(...) end
function PrintTable(t, indent)
    indent = indent or 0
    for k, v in pairs(t) do
        if type(v) == "table" and indent < 4 then print(string.rep(" ", indent) .. tostring(k) .. ":") PrintTable(v, indent + 2)
        else print(string.rep(" ", indent) .. tostring(k) .. " = " .. tostring(v)) end
    end
end

_G.__time = 0
function CurTime() return _G.__time end
RealTime = CurTime
SysTime = CurTime
function FrameTime() return 1 / 60 end
function engine_TickInterval() return 1 / 60 end

function IsValid(o) return o ~= nil and o ~= false and (type(o) ~= "table" or o.IsValid == nil or o:IsValid()) end
function istable(o) return type(o) == "table" end
function isfunction(o) return type(o) == "function" end
function isnumber(o) return type(o) == "number" end
function isstring(o) return type(o) == "string" end
function isbool(o) return type(o) == "boolean" end
function isentity(o) return type(o) == "table" and o.__entity == true end
function isvector(o) return type(o) == "table" and getmetatable(o) == _G.__VectorMT end
function isangle(o) return type(o) == "table" and getmetatable(o) == _G.__AngleMT end
function tobool(v) return v == true or v == 1 or v == "1" or v == "true" end
function Either(c, a, b) if c then return a end return b end
function unpack_safe(...) return unpack(...) end

function include(path)
    local candidates = { path }
    if _G.__current_dir then candidates[#candidates + 1] = _G.__current_dir .. path end
    for _, p in ipairs(candidates) do
        local src = __read_lua(p)
        if src then
            local prev = _G.__current_dir
            _G.__current_dir = p:match("^(.*/)") or ""
            local fn, err = loadstring(__gmod_preprocess(src), "@" .. p)
            if not fn then _G.__current_dir = prev error(err, 0) end
            local ok, res = pcall(fn)
            _G.__current_dir = prev
            if not ok then error(res, 0) end
            return res
        end
    end
    error("include: file not found: " .. path, 2)
end

---------------------------------------------------------------- libraries
file = { Exists = function(p) return __read_lua(p) ~= nil end, Find = function(pat) return __find_lua(pat) end, Read = function() return nil end, Write = noop, Append = noop, Open = function() return nil end, IsDir = function() return false end, CreateDir = noop, Delete = noop, Size = function() return 0 end, Time = function() return 0 end }
hook = { Add = noop, Remove = noop, Run = noop, Call = noop }
timer = { Simple = noop, Create = noop, Remove = noop, Exists = function() return false end, Destroy = noop }
concommand = { Add = noop }
-- Loopback net library: messages sent by the server code are handed straight to the registered receivers
-- (the host is both server and client).
do
    local receivers, out, inbox = {}, nil, nil
    local function writer() return function(v) if out then out[#out + 1] = v end end end
    local function reader() return function() if inbox then return table.remove(inbox, 1) end end end
    net = setmetatable({
        Receive = function(name, fn) receivers[name] = fn end,
        Start = function(name) out = { __name = name } return true end,
        WriteEntity = writer(), WriteString = writer(), WriteFloat = writer(), WriteUInt = writer(), WriteInt = writer(),
        WriteBool = writer(), WriteVector = writer(), WriteAngle = writer(), WriteTable = writer(), WriteDouble = writer(), WriteData = writer(),
        ReadEntity = reader(), ReadString = reader(), ReadFloat = reader(), ReadUInt = reader(), ReadInt = reader(),
        ReadBool = reader(), ReadVector = reader(), ReadAngle = reader(), ReadTable = reader(), ReadDouble = reader(),
    }, { __index = function() return noop end })
    local function send()
        local m = out
        out = nil
        local fn = m and not rawget(net, "Muted") and receivers[m.__name]
        if fn then
            inbox = m
            local ok, err = pcall(fn, #m, nil)
            inbox = nil
            if not ok then print("net " .. tostring(m.__name) .. ": " .. tostring(err)) end
        end
    end
    net.Broadcast, net.Send, net.SendPAS, net.SendPVS, net.SendOmit, net.SendToServer = send, send, send, send, send, send
end
util = setmetatable({}, { __index = function() return noop end })
util.AddNetworkString = noop
util.TraceLine = function() return { Hit = false, HitPos = Vector(0, 0, 0), Entity = nil } end
util.SharedRandom = function() return 0.5 end
util.IsValidModel = function() return true end
util.PrecacheModel = noop
util.PrecacheSound = noop
util.TypeToString = type
util.TableToJSON = function() return "{}" end
util.JSONToTable = function() return {} end
util.Base64Encode = function(s) return s end
util.CRC = function() return "0" end
resource = { AddFile = noop, AddWorkshop = noop, AddSingleFile = noop }
sound = setmetatable({}, { __index = function() return noop end })
language = { Add = noop }
list = setmetatable({ Get = function() return {} end }, { __index = function() return noop end })
duplicator = setmetatable({}, { __index = function() return noop end })
scripted_ents = setmetatable({ Get = function() return nil end, GetStored = function() return nil end }, { __index = function() return noop end })
effects = { Register = noop }
ents = { Create = function() return nil end, GetAll = function() return {} end, FindByClass = function() return {} end, FindInSphere = function() return {} end, GetByIndex = function() return nil end }
player = { GetAll = function() return {} end }
game = setmetatable({ GetMap = function() return "gm_metrostroi" end, SinglePlayer = function() return true end }, { __index = function() return noop end })
engine = setmetatable({ TickInterval = function() return 1 / 60 end, ActiveGamemode = function() return "sandbox" end }, { __index = function() return noop end })
system = { IsWindows = function() return true end, IsLinux = function() return false end }
cvars = { AddChangeCallback = noop, Number = function() return 0 end, Bool = function() return false end }
physenv = { AddSurfaceData = noop, GetPerformanceSettings = function() return {} end, SetPerformanceSettings = noop, GetGravity = function() return Vector(0, 0, -600) end }
surface = setmetatable({}, { __index = function() return noop end })
render = setmetatable({}, { __index = function() return noop end })
cam = setmetatable({}, { __index = function() return noop end })
draw = setmetatable({}, { __index = function() return noop end })
input = setmetatable({}, { __index = function() return noop end })
gui = setmetatable({}, { __index = function() return noop end })
vgui = setmetatable({}, { __index = function() return noop end })
derma = setmetatable({}, { __index = function() return noop end })
sql = setmetatable({}, { __index = function() return noop end })
Turbostroi = nil
constraint = setmetatable({}, { __index = function() return noop end })
undo = setmetatable({}, { __index = function() return noop end })
cleanup = setmetatable({}, { __index = function() return noop end })
usermessage = setmetatable({}, { __index = function() return noop end })
debugoverlay = setmetatable({}, { __index = function() return noop end })
navmesh = setmetatable({}, { __index = function() return noop end })
team = setmetatable({}, { __index = function() return noop end })
gamemode = setmetatable({}, { __index = function() return noop end })
http = setmetatable({}, { __index = function() return noop end })
steamworks = setmetatable({}, { __index = function() return noop end })
notification = setmetatable({}, { __index = function() return noop end })
killicon = setmetatable({}, { __index = function() return noop end })
properties = setmetatable({}, { __index = function() return noop end })
spawnmenu = setmetatable({}, { __index = function() return noop end })
baseclass = { Get = function() return {} end, Set = noop }
function DEFINE_BASECLASS() return {} end
function RunConsoleCommand() end
function Material() return {} end
function Sound(s) return s end
function ScrW() return 1920 end
function ScrH() return 1080 end
function LocalPlayer() return nil end
function CreateClientConVar(name, default) return __convar(name, default) end
function CreateConVar(name, default) return __convar(name, default) end
function GetConVar(name) return __convars[name] or __convar(name, 0) end
function GetGlobalInt(_, d) return d or 0 end
function GetGlobalFloat(_, d) return d or 0 end
function GetGlobalBool(_, d) return d or false end
function GetGlobalString(_, d) return d or "" end
SetGlobalInt = noop
SetGlobalFloat = noop
SetGlobalBool = noop
SetGlobalString = noop
function AccessorFunc(tbl, key, name)
    tbl["Get" .. name] = function(self) return self[key] end
    tbl["Set" .. name] = function(self, v) self[key] = v end
end
function ents_FindByClass() return {} end
function Format(...) return string.format(...) end
function Lerp(t, a, b) return a + (b - a) * t end
function LerpVector(t, a, b) return a + (b - a) * t end
function LerpAngle(t, a, b) return Angle(Lerp(t, a.p, b.p), Lerp(t, a.y, b.y), Lerp(t, a.r, b.r)) end
function Color(r, g, b, a) return { r = r, g = g, b = b, a = a or 255 } end
function ColorAlpha(c, a) c = c or Color(255, 255, 255) return Color(c.r, c.g, c.b, a) end
function FindMetaTable() return {} end
function SortedPairs(t, desc)
    local keys = {}
    for k in pairs(t) do keys[#keys + 1] = k end
    table.sort(keys, function(a, b) if desc then return a > b end return a < b end)
    local i = 0
    return function() i = i + 1 if keys[i] ~= nil then return keys[i], t[keys[i]] end end
end
function RandomPairs(t) return pairs(t) end
function Entity() return nil end
function Player() return nil end
function CompileString(code, id) return loadstring(__gmod_preprocess(code), id) end
function RunString(code, id) local f = loadstring(__gmod_preprocess(code), id) if f then f() end end

---------------------------------------------------------------- convars
__convars = {}
function __convar(name, default)
    local v = default
    local o = {}
    function o:GetInt() return math.floor(tonumber(v) or 0) end
    function o:GetFloat() return tonumber(v) or 0 end
    function o:GetBool() return v == true or tonumber(v) ~= 0 and v ~= "0" and v ~= false and v ~= nil end
    function o:GetString() return tostring(v) end
    function o:GetDefault() return default end
    function o:SetInt(x) v = x end
    o.SetFloat, o.SetBool, o.SetString = o.SetInt, o.SetInt, o.SetInt
    function o:GetName() return name end
    __convars[name] = o
    return o
end

---------------------------------------------------------------- math / table / string extensions
function math.Clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function math.Round(v, d) local m = 10 ^ (d or 0) return math.floor(v * m + 0.5) / m end
function math.Approach(cur, target, inc) inc = math.abs(inc) if cur < target then return math.min(cur + inc, target) end return math.max(cur - inc, target) end
function math.Rand(a, b) return a + math.random() * (b - a) end
function math.Remap(v, a, b, c, d) return c + (v - a) / (b - a) * (d - c) end
function math.Truncate(v, d) local m = 10 ^ (d or 0) return (v >= 0 and math.floor or math.ceil)(v * m) / m end
function math.NormalizeAngle(a) return (a + 180) % 360 - 180 end
function math.AngleDifference(a, b) local d = (a - b) % 360 if d > 180 then d = d - 360 end return d end
function math.Distance(x1, y1, x2, y2) return math.sqrt((x2 - x1) ^ 2 + (y2 - y1) ^ 2) end
function math.Sign(v) if v > 0 then return 1 elseif v < 0 then return -1 end return 0 end
math.sign = math.Sign
function math.Cap(v, lo, hi) return math.Clamp(v, lo, hi) end
function math.BinToInt(s) return tonumber(s, 2) end
function math.Min(...) return math.min(...) end
function math.Max(...) return math.max(...) end
function math.ceil_int(v) return math.ceil(v) end
function math.EaseInOut(f, _, _) return f * f * (3 - 2 * f) end
function math.SnapTo(v, s) return math.Round(v / s) * s end
function math.TimeFraction(a, b, v) return (v - a) / (b - a) end
function math.Fmod(a, b) return math.fmod(a, b) end
math.tau = math.pi * 2
if not math.log10 then math.log10 = function(x) return math.log(x) / math.log(10) end end

function table.Copy(t, lookup)
    if type(t) ~= "table" then return t end
    lookup = lookup or {}
    if lookup[t] then return lookup[t] end
    local c = {}
    lookup[t] = c
    for k, v in pairs(t) do c[k] = type(v) == "table" and table.Copy(v, lookup) or v end
    return setmetatable(c, getmetatable(t))
end
function table.Count(t) local n = 0 for _ in pairs(t) do n = n + 1 end return n end
function table.HasValue(t, v) for _, x in pairs(t) do if x == v then return true end end return false end
function table.KeyFromValue(t, v) for k, x in pairs(t) do if x == v then return k end end end
function table.Merge(dst, src) for k, v in pairs(src) do if type(v) == "table" and type(dst[k]) == "table" then table.Merge(dst[k], v) else dst[k] = v end end return dst end
function table.Empty(t) for k in pairs(t) do t[k] = nil end end
function table.Add(dst, src) for _, v in ipairs(src) do dst[#dst + 1] = v end return dst end
function table.Random(t) local keys = {} for k in pairs(t) do keys[#keys + 1] = k end local k = keys[math.random(#keys)] return t[k], k end
function table.GetKeys(t) local r = {} for k in pairs(t) do r[#r + 1] = k end return r end
function table.IsEmpty(t) return next(t) == nil end
function table.sortByKey(t) return t end
function table.Reverse(t) local r = {} for i = #t, 1, -1 do r[#r + 1] = t[i] end return r end
function table.RemoveByValue(t, v) for k, x in pairs(t) do if x == v then table.remove(t, k) return k end end end
function table.ClearKeys(t) local r = {} for _, v in pairs(t) do r[#r + 1] = v end return r end
function table.IsSequential(t) return #t == table.Count(t) end
function table.ToString(t) return tostring(t) end
function table.concat_keys() end

function string.Explode(sep, s, pat)
    local r, i = {}, 1
    while true do
        local a, b = string.find(s, sep, i, not pat)
        if not a then r[#r + 1] = s:sub(i) break end
        r[#r + 1] = s:sub(i, a - 1)
        i = b + 1
    end
    return r
end
string.Split = function(s, sep) return string.Explode(sep, s) end
function string.StartWith(s, p) return s:sub(1, #p) == p end
string.StartsWith = string.StartWith
function string.EndsWith(s, p) return p == "" or s:sub(-#p) == p end
function string.Trim(s, c) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.TrimLeft(s) return (s:gsub("^%s+", "")) end
function string.TrimRight(s) return (s:gsub("%s+$", "")) end
function string.Replace(s, a, b) return (s:gsub(a:gsub("%p", "%%%0"), (b:gsub("%%", "%%%%")))) end
function string.Left(s, n) return s:sub(1, n) end
function string.Right(s, n) return s:sub(-n) end
function string.ToTable(s) local t = {} for i = 1, #s do t[i] = s:sub(i, i) end return t end
function string.Comma(n) return tostring(n) end
function string.SetChar(s, i, c) return s:sub(1, i - 1) .. c .. s:sub(i + 1) end
function string.JavascriptSafe(s) return s end
function string.NiceSize(n) return tostring(n) end
function string.FormattedTime(s) return string.format("%02d:%02d", math.floor(s / 60), math.floor(s % 60)) end
function string.GetFileFromFilename(p) return p:match("[^/\\]*$") end
function string.StripExtension(p) return (p:gsub("%.[^.]*$", "")) end
function string.GetExtensionFromFilename(p) return p:match("%.([^.]*)$") end
function string.PatternSafe(s) return (s:gsub("%p", "%%%0")) end
function string.Implode(sep, t) return table.concat(t, sep) end
function string.find_plain(s, p) return s:find(p, 1, true) end
local smt = getmetatable("")
smt.__index = function(s, k) if type(k) == "number" then return string.sub(s, k, k) end return string[k] end

---------------------------------------------------------------- Vector / Angle
local VectorMT, AngleMT = {}, {}
_G.__VectorMT, _G.__AngleMT = VectorMT, AngleMT
local vec_alias = { r = "x", g = "y", b = "z", [1] = "x", [2] = "y", [3] = "z" }
VectorMT.__index = function(t, k)
    local m = VectorMT[k]
    if m ~= nil then return m end
    local a = vec_alias[k]
    if a then return rawget(t, a) end
end
VectorMT.__newindex = function(t, k, v) rawset(t, vec_alias[k] or k, v) end
local ang_alias = { [1] = "p", [2] = "y", [3] = "r", pitch = "p", yaw = "y", roll = "r" }
AngleMT.__index = function(t, k)
    local m = AngleMT[k]
    if m ~= nil then return m end
    local a = ang_alias[k]
    if a then return rawget(t, a) end
end
AngleMT.__newindex = function(t, k, v) rawset(t, ang_alias[k] or k, v) end

function Vector(x, y, z)
    if type(x) == "table" then return setmetatable({ x = x.x, y = x.y, z = x.z }, VectorMT) end
    return setmetatable({ x = x or 0, y = y or 0, z = z or 0 }, VectorMT)
end
function Angle(p, y, r)
    if type(p) == "table" then return setmetatable({ p = p.p, y = p.y, r = p.r }, AngleMT) end
    return setmetatable({ p = p or 0, y = y or 0, r = r or 0 }, AngleMT)
end
vector_origin = Vector(0, 0, 0)
angle_zero = Angle(0, 0, 0)

VectorMT.__add = function(a, b) return Vector(a.x + b.x, a.y + b.y, a.z + b.z) end
VectorMT.__sub = function(a, b) return Vector(a.x - b.x, a.y - b.y, a.z - b.z) end
VectorMT.__unm = function(a) return Vector(-a.x, -a.y, -a.z) end
VectorMT.__eq = function(a, b) return a.x == b.x and a.y == b.y and a.z == b.z end
VectorMT.__mul = function(a, b)
    if type(a) == "number" then return Vector(a * b.x, a * b.y, a * b.z) end
    if type(b) == "number" then return Vector(a.x * b, a.y * b, a.z * b) end
    return Vector(a.x * b.x, a.y * b.y, a.z * b.z)
end
VectorMT.__div = function(a, b) return Vector(a.x / b, a.y / b, a.z / b) end
VectorMT.__tostring = function(a) return string.format("%g %g %g", a.x, a.y, a.z) end
function VectorMT:Length() return math.sqrt(self.x ^ 2 + self.y ^ 2 + self.z ^ 2) end
function VectorMT:Length2D() return math.sqrt(self.x ^ 2 + self.y ^ 2) end
function VectorMT:LengthSqr() return self.x ^ 2 + self.y ^ 2 + self.z ^ 2 end
function VectorMT:Dot(o) return self.x * o.x + self.y * o.y + self.z * o.z end
function VectorMT:Cross(o) return Vector(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x) end
function VectorMT:Distance(o) return (self - o):Length() end
function VectorMT:DistToSqr(o) return (self - o):LengthSqr() end
function VectorMT:GetNormalized() local l = self:Length() if l == 0 then return Vector(0, 0, 0) end return self / l end
VectorMT.GetNormal = VectorMT.GetNormalized
function VectorMT:Normalize() local l = self:Length() if l > 0 then self.x, self.y, self.z = self.x / l, self.y / l, self.z / l end end
function VectorMT:Set(o) self.x, self.y, self.z = o.x, o.y, o.z end
function VectorMT:Add(o) self.x, self.y, self.z = self.x + o.x, self.y + o.y, self.z + o.z end
function VectorMT:Sub(o) self.x, self.y, self.z = self.x - o.x, self.y - o.y, self.z - o.z end
function VectorMT:Mul(n) self.x, self.y, self.z = self.x * n, self.y * n, self.z * n end
function VectorMT:Zero() self.x, self.y, self.z = 0, 0, 0 end
function VectorMT:IsZero() return self.x == 0 and self.y == 0 and self.z == 0 end
function VectorMT:Unpack() return self.x, self.y, self.z end
function VectorMT:Angle()
    local yaw = math.deg(math.atan2(self.y, self.x))
    local pitch = math.deg(math.atan2(-self.z, math.sqrt(self.x ^ 2 + self.y ^ 2)))
    return Angle(pitch, yaw, 0)
end
local function rotmat(a)
    local cp, sp = math.cos(math.rad(a.p)), math.sin(math.rad(a.p))
    local cy, sy = math.cos(math.rad(a.y)), math.sin(math.rad(a.y))
    local cr, sr = math.cos(math.rad(a.r)), math.sin(math.rad(a.r))
    return {
        { cp * cy, sp * sr * cy - cr * sy, sp * cr * cy + sr * sy },
        { cp * sy, sp * sr * sy + cr * cy, sp * cr * sy - sr * cy },
        { -sp, sr * cp, cr * cp },
    }
end
function VectorMT:Rotate(a)
    local m = rotmat(a)
    local x, y, z = self.x, self.y, self.z
    self.x = m[1][1] * x + m[1][2] * y + m[1][3] * z
    self.y = m[2][1] * x + m[2][2] * y + m[2][3] * z
    self.z = m[3][1] * x + m[3][2] * y + m[3][3] * z
end
function AngleMT:Forward() local m = rotmat(self) return Vector(m[1][1], m[2][1], m[3][1]) end
function AngleMT:Right() local m = rotmat(self) return Vector(-m[1][2], -m[2][2], -m[3][2]) end
function AngleMT:Up() local m = rotmat(self) return Vector(m[1][3], m[2][3], m[3][3]) end
AngleMT.__add = function(a, b) return Angle(a.p + b.p, a.y + b.y, a.r + b.r) end
AngleMT.__sub = function(a, b) return Angle(a.p - b.p, a.y - b.y, a.r - b.r) end
AngleMT.__eq = function(a, b) return a.p == b.p and a.y == b.y and a.r == b.r end
AngleMT.__mul = function(a, b) if type(a) == "number" then a, b = b, a end return Angle(a.p * b, a.y * b, a.r * b) end
AngleMT.__tostring = function(a) return string.format("%g %g %g", a.p, a.y, a.r) end
function AngleMT:Set(o) self.p, self.y, self.r = o.p, o.y, o.r end
function AngleMT:Normalize() self.p, self.y, self.r = math.NormalizeAngle(self.p), math.NormalizeAngle(self.y), math.NormalizeAngle(self.r) end
function AngleMT:RotateAroundAxis() return self end
function AngleMT:Unpack() return self.p, self.y, self.r end
function AngleMT:IsZero() return self.p == 0 and self.y == 0 and self.r == 0 end
function LocalToWorld(lp, la, op, oa) local v = Vector(lp) v:Rotate(oa) return op + v, la end
function WorldToLocal(wp, wa, op, oa) return wp - op, wa end
function VectorRand() return Vector(math.random() * 2 - 1, math.random() * 2 - 1, math.random() * 2 - 1) end

---------------------------------------------------------------- Metrostroi core (systems registry)
Metrostroi = Metrostroi or {}
Metrostroi.Systems = Metrostroi.Systems or {}
Metrostroi.BaseSystems = Metrostroi.BaseSystems or {}
Metrostroi.TurbostroiRegistered = Metrostroi.TurbostroiRegistered or {}

function Metrostroi.DefineSystem(name)
    if not Metrostroi.BaseSystems[name] then Metrostroi.BaseSystems[name] = {} end
    TRAIN_SYSTEM = Metrostroi.BaseSystems[name]
    TRAIN_SYSTEM_NAME = name
end

function Metrostroi.LoadSystemFile(filename)
    TRAIN_SYSTEM, TRAIN_SYSTEM_NAME = nil, nil
    include(filename)
    if TRAIN_SYSTEM then TRAIN_SYSTEM.FileName = filename end
    local name = TRAIN_SYSTEM_NAME or "UndefinedSystem"
    TRAIN_SYSTEM_NAME = nil

    Metrostroi.Systems["_" .. name] = TRAIN_SYSTEM
    Metrostroi.BaseSystems[name] = TRAIN_SYSTEM
    Metrostroi.Systems[name] = function(train, ...)
        local tbl = { _base = name }
        local base = Metrostroi.BaseSystems[tbl._base]
        if not base then print("No system: " .. tbl._base) return end
        for k, v in pairs(base) do
            if type(v) == "function" then
                tbl[k] = function(...) return Metrostroi.BaseSystems[tbl._base][k](...) end
            else
                tbl[k] = v
            end
        end
        tbl.Initialize = tbl.Initialize or function() end
        tbl.Think = tbl.Think or function() end
        tbl.Inputs = tbl.Inputs or function() return {} end
        tbl.Outputs = tbl.Outputs or function() return {} end
        tbl.TriggerOutput = tbl.TriggerOutput or function() end
        tbl.TriggerInput = tbl.TriggerInput or function() end
        tbl.Train = train
        tbl:Initialize(...)
        tbl.OutputsList = tbl:Outputs()
        tbl.InputsList = tbl:Inputs()
        tbl.IsInput = {}
        for _, v in pairs(tbl.InputsList) do tbl.IsInput[v] = true end
        return tbl
    end
    return name
end

---------------------------------------------------------------- more engine globals
function SafeRemoveEntity(e) if e and e.Remove then e:Remove() end end
function GetConVarNumber(n) local c = __convars[n] return c and c:GetFloat() or 0 end
function GetConVarString(n) local c = __convars[n] return c and c:GetString() or "" end
function IsMounted() return false end
function IsFirstTimePredicted() return true end
function IsUselessModel() return false end
function CPPI() end
function Wire_TriggerOutput() end
function EmitSound() end
function SoundDuration() return 1 end
function ParticleEffect() end
function ParticleEffectAttach() end
function GetHostName() return "metrostroi-rs" end
function VectorRand2() return Vector(0, 0, 0) end
function ents_Create() end
function WireLib() end
function ProtectedCall(f, ...) return pcall(f, ...) end
function SafeRemove(e) SafeRemoveEntity(e) end
function Msg_(...) end
function MsgC(...) end
function print_table(t) PrintTable(t) end
function GetRenderTarget() return {} end
function GetRenderTargetEx() return {} end
function CreateMaterial() return {} end
function Mesh() return {} end
function Matrix() return setmetatable({}, { __index = function() return noop end }) end
function DynamicLight() return {} end
function CurTimeSync() return CurTime() end
function SoundIsPlaying() return false end
function CreateSound() return setmetatable({}, { __index = function() return noop end }) end
function RealFrameTime() return 1 / 60 end
function UnPredictedCurTime() return CurTime() end
function PrecacheParticleSystem() end
function Material_() end
function ClientsideModel() return nil end
function ClientsideRagdoll() return nil end
function ClientsideScene() return nil end
function GetViewEntity() return nil end
function EyePos() return Vector(0, 0, 0) end
function EyeAngles() return Angle(0, 0, 0) end
function EyeVector() return Vector(1, 0, 0) end
function ScreenScale(n) return n end
function SetClipboardText() end
function OpenProfile() end
function ChangeTooltip() end
function GetTimeoutInfo() return false, 0 end
function ColorToHSV(c) return 0, 0, 0 end
function HSVToColor() return Color(255, 255, 255) end
function Add_NPC_Class() end
function RecipientFilter() return setmetatable({}, { __index = function() return noop end }) end
function BroadcastLua() end
function NumModelSkins() return 1 end
function ents_FindInSphere() return {} end
function VectorToLocalSpace(v) return v end
function SuppressHostEvents() end
function IsEntity(e) return isentity(e) end
function Error_(...) end
function tostringsafe(v) return tostring(v) end
function ValidPanel() return false end
function DermaMenu() return setmetatable({}, { __index = function() return noop end }) end
function GetGlobal2Int(_, d) return d or 0 end
SetGlobal2Int = noop
GetGlobal2Float = GetGlobalFloat
SetGlobal2Float = noop
GetGlobal2Bool = GetGlobalBool
SetGlobal2Bool = noop
GetGlobal2String = GetGlobalString
SetGlobal2String = noop
for _, k in ipairs({ "RENDERMODE_TRANSALPHA", "RENDERMODE_NORMAL", "RENDERMODE_TRANSCOLOR", "RENDERMODE_GLOW", "RENDERMODE_WORLDGLOW", "RENDERMODE_TRANSADD" }) do _G[k] = 0 end
for k, v in pairs({ SOLID_VPHYSICS = 6, SOLID_NONE = 0, SOLID_BBOX = 2, MOVETYPE_VPHYSICS = 6, MOVETYPE_NONE = 0, MOVETYPE_NOCLIP = 8, SIMPLE_USE = 3, CONTINUOUS_USE = 1, COLLISION_GROUP_WORLD = 20,
    COLLISION_GROUP_NONE = 0, COLLISION_GROUP_DEBRIS = 1, COLLISION_GROUP_WEAPON = 11, COLLISION_GROUP_PASSABLE_DOOR = 5, COLLISION_GROUP_INTERACTIVE_DEBRIS = 3, FCVAR_ARCHIVE = 128, FCVAR_NOTIFY = 256, FCVAR_REPLICATED = 8192, FCVAR_USERINFO = 512, FCVAR_NONE = 0,
    FCVAR_UNREGISTERED = 1, FCVAR_PRINTABLEONLY = 1024, FCVAR_CHEAT = 16384, FCVAR_DONTRECORD = 131072, FCVAR_SERVER_CAN_EXECUTE = 268435456,
    MASK_SOLID = 33570827, MASK_SOLID_BRUSHONLY = 16395, MASK_PLAYERSOLID = 33636363, MASK_NPCWORLDSTATIC = 131083, MASK_SHOT = 1174421507, MASK_OPAQUE = 16513, MASK_ALL = -1, CONTENTS_SOLID = 1,
    TEXT_ALIGN_LEFT = 0, TEXT_ALIGN_CENTER = 1, TEXT_ALIGN_RIGHT = 2, TEXT_ALIGN_TOP = 3, TEXT_ALIGN_BOTTOM = 4, EFL_KEEP_ON_RECREATE_ENTITIES = 16384, EFL_DONTBLOCKLOS = 64,
    SF_PHYSPROP_MOTIONDISABLED = 8, SF_PHYSPROP_ENABLE_ON_PHYSCANNON = 64, FL_FROZEN = 64, FL_GODMODE = 8192, FL_NOTARGET = 128, NOTSOLID = 0, ACT_IDLE = 0,
    BLEND_ZERO = 0, BLEND_ONE = 1, BLEND_SRC_ALPHA = 4, BLEND_ONE_MINUS_SRC_ALPHA = 5,
    PLAYER_IDLE = 0, FORCE_NUMBER = 1, FORCE_STRING = 2, FORCE_BOOL = 3, SENSORBONE = 0,
    OBS_MODE_NONE = 0, ROUND_STARTED = 1, NODE_TYPE_GROUND = 2, KEY_NONE = 0, MOUSE_LEFT = 107, MOUSE_RIGHT = 108,
    EVENT_CLIENT = 1, OBS_MODE_IN_EYE = 4, USE_ON = 1, USE_OFF = 0, USE_TOGGLE = 2, USE_SET = 3,
    IN_ATTACK = 1, IN_JUMP = 2, IN_DUCK = 4, IN_FORWARD = 8, IN_BACK = 16, IN_USE = 32, IN_LEFT = 128, IN_RIGHT = 256, IN_MOVELEFT = 512, IN_MOVERIGHT = 1024, IN_ATTACK2 = 2048, IN_RELOAD = 8192, IN_SPEED = 131072, IN_WALK = 262144,
    TYPE_TABLE = 5, TYPE_NUMBER = 3, TYPE_STRING = 4, TYPE_BOOL = 1, TYPE_ENTITY = 9, TYPE_NIL = 0, TYPE_VECTOR = 10, TYPE_ANGLE = 11, TYPE_FUNCTION = 6,
    TRANSMIT_ALWAYS = 0, TRANSMIT_PVS = 1, TRANSMIT_NEVER = 2, NUM_DOORS = 4, BONE_USED_BY_VERTEX_LOD0 = 256, ENTS = 0, PATTACH_ABSORIGIN = 0, PATTACH_ABSORIGIN_FOLLOW = 1, PATTACH_POINT = 2, PATTACH_POINT_FOLLOW = 4 }) do _G[k] = v end

-- keyboard constants (KEY_*)
do
    local n = 1
    for c = 0, 9 do _G["KEY_" .. c] = n n = n + 1 end
    for c = string.byte("A"), string.byte("Z") do _G["KEY_" .. string.char(c)] = n n = n + 1 end
    for i = 0, 9 do _G["KEY_PAD_" .. i] = n n = n + 1 end
    for _, k in ipairs({ "PAD_DIVIDE", "PAD_MULTIPLY", "PAD_MINUS", "PAD_PLUS", "PAD_ENTER", "PAD_DECIMAL", "LBRACKET", "RBRACKET", "SEMICOLON", "APOSTROPHE", "BACKQUOTE", "COMMA", "PERIOD", "SLASH", "BACKSLASH", "MINUS", "EQUAL", "ENTER", "SPACE", "BACKSPACE", "TAB", "CAPSLOCK", "NUMLOCK", "ESCAPE", "SCROLLLOCK", "INSERT", "DELETE", "HOME", "END", "PAGEUP", "PAGEDOWN", "BREAK", "LSHIFT", "RSHIFT", "LALT", "RALT", "LCONTROL", "RCONTROL", "LWIN", "RWIN", "APP", "UP", "LEFT", "DOWN", "RIGHT" }) do
        _G["KEY_" .. k] = n n = n + 1
    end
    for i = 1, 12 do _G["KEY_F" .. i] = n n = n + 1 end
end
Metrostroi.TrainClasses = Metrostroi.TrainClasses or {}
Metrostroi.Skins = Metrostroi.Skins or { ["717_schemes"] = { p = {}, m = {} } }
do
    local insert = table.insert
    function table.insert(t, a, b)
        if b ~= nil then insert(t, a, b) return a end
        insert(t, a)
        return #t
    end
end
for _, n in ipairs({ "matproxy", "sound", "chat", "halo", "weapons", "presets", "menubar", "contentcloud", "search", "saverestore", "markup", "dragndrop", "ai", "ai_task", "numpad", "drive", "widgets", "ents_client", "motionsensor", "video", "permissions", "bit_" }) do
    if _G[n] == nil then _G[n] = setmetatable({}, { __index = function() return noop end }) end
end
