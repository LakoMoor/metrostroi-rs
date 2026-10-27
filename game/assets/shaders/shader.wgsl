struct Globals {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var smp: sampler;
struct Object { model: mat4x4<f32>, tint: vec4<f32> };
@group(2) @binding(0) var<uniform> obj: Object;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> VOut {
    var o: VOut;
    o.pos = g.view_proj * (obj.model * vec4<f32>(pos, 1.0));
    o.normal = (obj.model * vec4<f32>(normal, 0.0)).xyz;
    o.uv = uv;
    return o;
}

fn shade(i: VOut, c: vec4<f32>) -> vec3<f32> {
    if (g.light.w > 0.5) {
        return c.rgb * obj.tint.rgb;
    }
    // sky/floor hemisphere ambient plus one directional light, scaled by the object's tint (lamp glow)
    let n = normalize(i.normal);
    let hemi = mix(0.45, 0.8, n.z * 0.5 + 0.5);
    let l = hemi + max(dot(n, normalize(g.light.xyz)), 0.0) * 0.45;
    return min(c.rgb * l * 1.5, vec3<f32>(1.0)) * obj.tint.rgb;
}

// Source stores unrelated data (specular, masks) in alpha of opaque materials: ignore it.
@fragment
fn fs_opaque(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, smp, i.uv);
    return vec4<f32>(shade(i, c), 1.0);
}

@fragment
fn fs_alphatest(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, smp, i.uv);
    if (c.a < 0.5) {
        discard;
    }
    return vec4<f32>(shade(i, c), 1.0);
}

@fragment
fn fs_blend(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, smp, i.uv);
    return vec4<f32>(shade(i, c), c.a);
}
