struct Screen { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) lp: vec2<f32>,
    @location(4) shape: vec4<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) lp: vec2<f32>,
    @location(3) shape: vec4<f32>,
};

@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(v.pos.x / screen.size.x * 2.0 - 1.0, 1.0 - v.pos.y / screen.size.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.lp = v.lp;
    o.shape = v.shape;
    return o;
}

@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
    if (i.shape.w > 0.5) {
        // rounded box (circles and capsules are rounded boxes too): half size in shape.xy, radius in shape.z
        let q = abs(i.lp) - (i.shape.xy - vec2<f32>(i.shape.z));
        let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - i.shape.z;
        let a = clamp(0.5 - d, 0.0, 1.0);
        return vec4<f32>(i.color.rgb, i.color.a * a);
    }
    let t = textureSample(atlas, samp, i.uv);
    return vec4<f32>(i.color.rgb, i.color.a * t.a);
}
