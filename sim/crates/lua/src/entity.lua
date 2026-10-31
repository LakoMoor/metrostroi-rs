-- Scripted-entity host: loads entities/<class>/{shared,init}.lua with GMod's ENT inheritance and
-- spawns instances that stand in for engine entities (no physics, no rendering).
local noop = function() end

Entity_MT = {}
Entity_MT.__index = function(t, k)
    local cls = rawget(t, "__class")
    while cls do
        local v = rawget(cls, k)
        if v ~= nil then return v end
        cls = rawget(cls, "__base")
    end
    return Entity_MT[k]
end

local nw = function(self) self.__nw = self.__nw or {} return self.__nw end
local function getter(default) return function(self, key, d) local v = nw(self)[key] if v == nil then if d ~= nil then return d end return default end return v end end
local function setter(self, key, v) nw(self)[key] = v end
for _, suffix in ipairs({ "Float", "Int", "Bool", "String", "Entity", "Vector", "Angle" }) do
    local default = ({ Float = 0, Int = 0, Bool = false, String = "", Entity = nil, Vector = Vector(0, 0, 0), Angle = Angle(0, 0, 0) })[suffix]
    for _, prefix in ipairs({ "NW2", "NW", "" }) do
        Entity_MT["Get" .. prefix .. suffix] = getter(default)
        -- a Lua 0 is truthy, but the engine stores the bool; systems write 0 meaning "off"
        Entity_MT["Set" .. prefix .. suffix] = suffix == "Bool" and function(self, key, v) if v == 0 then v = false end nw(self)[key] = v and true or false end or setter
    end
end

Entity_MT.__entity = true
function Entity_MT:IsValid() return not self.__removed end
function Entity_MT:EntIndex() return self.__index_id end
function Entity_MT:GetClass() return self.ClassName end
function Entity_MT:GetPos() return self.__pos end
function Entity_MT:SetPos(p) self.__pos = p end
function Entity_MT:GetAngles() return self.__ang end
function Entity_MT:SetAngles(a) self.__ang = a end
function Entity_MT:GetVelocity() return Vector(0, 0, 0) end
function Entity_MT:GetModel() return self.__model or "models/error.mdl" end
function Entity_MT:SetModel(m) self.__model = m end
function Entity_MT:LocalToWorld(v) return self.__pos + v end
function Entity_MT:WorldToLocal(v) return v - self.__pos end
function Entity_MT:LocalToWorldAngles(a) return a end
function Entity_MT:WorldToLocalAngles(a) return a end
function Entity_MT:GetUp() return Vector(0, 0, 1) end
function Entity_MT:GetForward() return Vector(1, 0, 0) end
function Entity_MT:GetRight() return Vector(0, -1, 0) end
function Entity_MT:GetOwner() return nil end
function Entity_MT:GetColor() return Color(255, 255, 255, 255) end
function Entity_MT:GetPhysicsObject() return Entity_MT.__phys end
function Entity_MT:GetSkin() return 0 end
function Entity_MT:SkinCount() return 1 end
function Entity_MT:GetBodygroup() return 0 end
function Entity_MT:OnGround() return true end
function Entity_MT:WaterLevel() return 0 end
function Entity_MT:Alive() return true end
function Entity_MT:Health() return 100 end
function Entity_MT:GetNumBodyGroups() return 0 end
function Entity_MT:GetFlexNum() return 0 end
function Entity_MT:LookupAttachment() return 0 end
function Entity_MT:LookupSequence() return -1 end
function Entity_MT:GetBoneCount() return 0 end
function Entity_MT:LookupBone() return nil end
function Entity_MT:GetBonePosition() return Vector(0, 0, 0), Angle(0, 0, 0) end
function Entity_MT:OBBMins() return Vector(-64, -64, -64) end
function Entity_MT:OBBMaxs() return Vector(64, 64, 64) end
function Entity_MT:OBBCenter() return Vector(0, 0, 0) end
function Entity_MT:BoundingRadius() return 100 end
function Entity_MT:GetCreationTime() return 0 end
function Entity_MT:GetTable() return self end
function Entity_MT:IsPlayer() return false end
function Entity_MT:IsWorld() return false end
function Entity_MT:IsVehicle() return false end
function Entity_MT:IsNPC() return false end
function Entity_MT:Remove() self.__removed = true end
function Entity_MT:CPPISetOwner() end
function Entity_MT:Spawn() if self.Initialize then self:Initialize() end end
function Entity_MT:Activate() end
function Entity_MT:__tostring() return "Entity[" .. tostring(self.ClassName) .. "]" end
function Entity_MT:NextThink() end
function Entity_MT:GetCollisionGroup() return 0 end
function Entity_MT:GetParent() return nil end
function Entity_MT:IsEFlagSet() return false end
function Entity_MT:IsOnFire() return false end
function Entity_MT:IsConstraint() return false end
function Entity_MT:GetMaterial() return "" end
function Entity_MT:GetMaterials() return {} end
function Entity_MT:GetMaxHealth() return 100 end
function Entity_MT:GetGravity() return 1 end
function Entity_MT:GetSolid() return 0 end
function Entity_MT:GetMoveType() return 0 end
function Entity_MT:GetName() return "" end
function Entity_MT:GetSequence() return 0 end
function Entity_MT:GetCycle() return 0 end
function Entity_MT:GetPlaybackRate() return 1 end
function Entity_MT:GetNumPoseParameters() return 0 end
function Entity_MT:GetModelRadius() return 100 end
function Entity_MT:SetNetworkedBool(k, v) setter(self, k, v) end

for _, name in ipairs({
    "SetRenderMode", "SetColor", "PhysicsInit", "SetMoveType", "SetSolid", "SetUseType", "SetSkin", "SetBodygroup",
    "SetOwner", "SetParent", "SetCollisionGroup", "SetNoDraw", "SetModelScale", "SetMaterial", "SetPlaybackRate",
    "SetSequence", "SetCycle", "SetPoseParameter", "SetFlexWeight", "SetNotSolid", "SetTrigger", "SetKeyValue",
    "SetCustomCollisionCheck", "SetSubMaterial", "SetLagCompensated", "SetTransmitWithParent", "SetLocalPos",
    "SetLocalAngles", "SetGravity", "SetHealth", "SetName", "SetShouldServerRagdoll", "SetPersistent", "AddEFlags",
    "RemoveEFlags", "DrawShadow", "EmitSound", "StopSound", "EnableCustomCollisions", "PhysWake", "SetupBones",
    "ManipulateBoneAngles", "ManipulateBonePosition", "ManipulateBoneScale", "FollowBone", "AddCallback",
    "DeleteOnRemove", "CallOnRemove", "SetAngularVelocity", "SetVelocity", "SetUnFreezable", "SetNW2VarProxy",
    "NetworkVar", "NetworkVarNotify", "SetupDataTables", "SetMaxHealth", "AddFlags", "RemoveFlags", "SetPreventTransmit",
    "AddSolidFlags", "RemoveSolidFlags", "SetCollisionBounds", "SetEntity", "DontDeleteOnRemove", "CollisionRulesChanged",
    "SetFriction", "SetElasticity", "UseTriggerBounds", "SetLocalVelocity", "SetAbsVelocity", "ResetSequence", "SetBoneMatrix",
    "SetCreator", "ClearPoseParameters", "SetAnimation", "PhysicsDestroy", "SetBodyGroups", "AddCallback", "RemoveCallOnRemove",
}) do Entity_MT[name] = Entity_MT[name] or noop end

-- a physics object stand-in
local phys = setmetatable({}, { __index = function(_, k)
    if k == "IsValid" then return function() return true end end
    if k == "GetVelocity" or k == "GetAngleVelocity" or k == "GetPos" then return function() return Vector(0, 0, 0) end end
    if k == "GetMass" then return function() return 1000 end end
    return noop
end })
Entity_MT.__phys = phys

---------------------------------------------------------------- class loading
local classes = {}
local function load_file(path, ENT)
    local prev = _G.ENT
    _G.ENT = ENT
    local ok, err = pcall(include, path)
    _G.ENT = prev
    return ok, err
end

function Metrostroi.GetEntityClass(name)
    if classes[name] then return classes[name] end
    local cls = { ClassName = name }
    cls.__index = nil
    classes[name] = cls
    local dir = "entities/" .. name .. "/"
    local ok, err = load_file(dir .. "shared.lua", cls)
    if not ok then
        if not __read_lua(dir .. "shared.lua") then
            ok, err = load_file("entities/" .. name .. ".lua", cls)
        end
        if not ok then error("entity " .. name .. ": " .. tostring(err), 0) end
    end
    local ok2, err2 = load_file(dir .. "init.lua", cls)
    if not ok2 and __read_lua(dir .. "init.lua") then error("entity " .. name .. " init.lua: " .. tostring(err2), 0) end
    if cls.Base and cls.Base ~= name then
        cls.__base = Metrostroi.GetEntityClass(cls.Base)
        cls.BaseClass = cls.__base
    end
    return cls
end

local next_index = 1
function Metrostroi.CreateEntity(name)
    local cls = Metrostroi.GetEntityClass(name)
    local e = setmetatable({ __class = cls, __pos = Vector(0, 0, 0), __ang = Angle(0, 0, 0), ClassName = name }, Entity_MT)
    e.__index_id = next_index
    next_index = next_index + 1
    e:SetupDataTables()
    return e
end

function ents.Create(name)
    local ok, e = pcall(Metrostroi.CreateEntity, name)
    if ok then return e end
    -- helper entities (seats, bogeys, couples...) are not simulated: hand back an inert stand-in
    local d = setmetatable({ __class = {}, __pos = Vector(0, 0, 0), __ang = Angle(0, 0, 0), ClassName = name }, Entity_MT)
    d.__index_id = next_index
    next_index = next_index + 1
    return d
end

ents.GetAll = function() return {} end
function Metrostroi.SpawnTrain(name)
    local e = Metrostroi.CreateEntity(name)
    e:Initialize()
    return e
end

function Entity_MT:CPPIGetOwner() return nil end
function Entity_MT:GetDriver() return nil end
function Entity_MT:GetPassenger() return nil end
function Entity_MT:GetThirdPersonMode() return false end
function Entity_MT:GetCameraDistance() return 0 end
function Entity_MT:GetVehicleClass() return "" end
function Entity_MT:GetVehicleViewPosition() return Vector(0, 0, 0), Angle(0, 0, 0) end
function Entity_MT:GetAttachment() return nil end
function Entity_MT:GetBoneMatrix() return nil end
function Entity_MT:GetPhysicsObjectCount() return 0 end
function Entity_MT:GetPhysicsObjectNum() return phys end
function Entity_MT:GetChildren() return {} end
function Entity_MT:GetSaveTable() return {} end
function Entity_MT:Fire() end
function Entity_MT:Input() end
function Entity_MT:Extinguish() end
function Entity_MT:Ignite() end
function Entity_MT:TakeDamage() end
function Entity_MT:GetBonePosition() return Vector(0, 0, 0), Angle(0, 0, 0) end

function Entity_MT:GetLocalAngles() return Angle(0, 0, 0) end
function Entity_MT:GetLocalPos() return Vector(0, 0, 0) end
function Entity_MT:GetAbsVelocity() return Vector(0, 0, 0) end
function Entity_MT:GetAngleVelocity() return Vector(0, 0, 0) end
function Entity_MT:GetBaseVelocity() return Vector(0, 0, 0) end
function Entity_MT:GetMoveParent() return nil end
function Entity_MT:GetGroundEntity() return nil end
function Entity_MT:NearestPoint(p) return p end
function Entity_MT:WorldSpaceCenter() return self.__pos end
function Entity_MT:WorldSpaceAABB() return Vector(-1, -1, -1), Vector(1, 1, 1) end
function Entity_MT:GetRenderMode() return 0 end
function Entity_MT:GetUseType() return 0 end
function Entity_MT:GetSpawnTime() return 0 end
function Entity_MT:GetOwnerEntity() return nil end
function Entity_MT:MapCreationID() return -1 end
function Entity_MT:IsDormant() return false end
function Entity_MT:InVehicle() return false end
function Entity_MT:Visible() return true end
function Entity_MT:TranslateBoneToPhysBone() return 0 end
function Entity_MT:IsMarkedForDeletion() return false end
function Entity_MT:Distance(o) return 0 end
function Entity_MT:GetPos2() return self.__pos end

---------------------------------------------------------------- client side (cab animation)
-- The cab's own client code (cl_init.lua) animates props through Animate()/SetPoseParameter()
-- on client-side models. Those models are inert mocks that just record what was asked of them.
local client_classes = {}
local function with_side(client, fn, ...)
    local s, c = SERVER, CLIENT
    SERVER, CLIENT = not client, client
    local r = { pcall(fn, ...) }
    SERVER, CLIENT = s, c
    if not r[1] then error(r[2], 0) end
    return unpack(r, 2)
end

function Metrostroi.GetClientClass(name)
    if client_classes[name] then return client_classes[name] end
    local cls = { ClassName = name }
    client_classes[name] = cls
    with_side(true, function()
        if not Metrostroi.__cl_util then
            Metrostroi.__cl_util = true
            include("metrostroi/cl_util.lua")
        end
        local dir = "entities/" .. name .. "/"
        local ok, err = load_file(dir .. "shared.lua", cls)
        if not ok then error("client " .. name .. " shared: " .. tostring(err), 0) end
        local ok2, err2 = load_file(dir .. "cl_init.lua", cls)
        if not ok2 and __read_lua(dir .. "cl_init.lua") then error("client " .. name .. " cl_init: " .. tostring(err2), 0) end
        if cls.Base and cls.Base ~= name then
            cls.__base = Metrostroi.GetClientClass(cls.Base)
            cls.BaseClass = cls.__base
        end
    end)
    return cls
end

local MockModel_MT = {}
MockModel_MT.__index = function(t, k)
    local v = rawget(MockModel_MT, k)
    if v ~= nil then return v end
    return Entity_MT[k]
end
MockModel_MT.__tostring = function() return "MockModel" end
MockModel_MT.__entity = true
function MockModel_MT:SetPoseParameter(name, v) self.__pose[name] = v end
function MockModel_MT:GetPoseParameter(name) return self.__pose[name] or 0 end
function MockModel_MT:SetBodygroup(i, v) self.__bodygroups[i] = v end
function MockModel_MT:SetSkin(v) self.__skin = v end
function MockModel_MT:SetNoDraw(v) self.__nodraw = v end
function MockModel_MT:SetColor(c) self.__color = c end
function MockModel_MT:GetModel() return self.__model end
function MockModel_MT:Remove() self.__removed = true end
function MockModel_MT:IsValid() return not self.__removed end
function MockModel_MT:SetModel(m) self.__model = m end
function MockModel_MT:SetPos(p) self.__pos = p end
function MockModel_MT:SetAngles(a) self.__ang = a end
function MockModel_MT:SetLocalPos(p) self.__pos = p end
function MockModel_MT:SetLocalAngles(a) self.__ang = a end
function MockModel_MT:GetPos() return self.__pos end
function MockModel_MT:GetAngles() return self.__ang end
function MockModel_MT:GetMaterials() return {} end

function ClientsideModel(model)
    return setmetatable({ __model = model, __pose = {}, __bodygroups = {}, __pos = Vector(0, 0, 0), __ang = Angle(0, 0, 0), __nw = {} }, MockModel_MT)
end

-- Client twin of a server entity: same networked values, runs cl_init code.
function Metrostroi.CreateClient(server)
    local cls = Metrostroi.GetClientClass(server.ClassName)
    local e = setmetatable({ __class = cls, __pos = server.__pos, __ang = server.__ang, ClassName = server.ClassName }, Entity_MT)
    e.__nw = server.__nw or {}
    server.__nw = e.__nw
    e.__index_id = server.__index_id
    e.__server = server
    -- the cab is always "near": render all props, no distance culling
    e.ShouldRenderClientEnts = function() return true end
    e.ShouldDrawClientEnt = function() return true end
    with_side(true, function()
        MetrostroiStarted = true
        e:SetupDataTables()
        e:Initialize()
    end)
    return e
end

function Metrostroi.ClientThink(e, dt)
    with_side(true, function()
        _G.__frame_time = dt
        e:Think()
    end)
end
function FrameTime() return _G.__frame_time or 1 / 60 end

-- the viewer's own player: sitting nowhere, standing in the cab
local LocalPlayer_obj = setmetatable({ __entity = true }, { __index = function(_, k)
    if k == "IsValid" or k == "Alive" or k == "IsPlayer" then return function() return true end end
    if k == "GetPos" or k == "EyePos" or k == "GetShootPos" or k == "GetVelocity" then return function() return Vector(0, 0, 0) end end
    if k == "EyeAngles" or k == "GetAngles" then return function() return Angle(0, 0, 0) end end
    if k == "GetAimVector" then return function() return Vector(1, 0, 0) end end
    if k == "GetVehicle" or k == "GetActiveWeapon" or k == "GetEyeTrace" then return function() return nil end end
    if k == "GetInfoNum" then return function(_, _, d) return d or 0 end end
    return noop
end })
function LocalPlayer() return LocalPlayer_obj end
