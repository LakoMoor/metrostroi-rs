// Post-processing for the ultra tier: bright-pass, separable blur at quarter resolution, composite with ACES.
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var bloom: texture_2d<f32>;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    var o: V;
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }

// 4-tap bilinear downsample with a soft threshold
@fragment
fn fs_bright(v: V) -> @location(0) vec4<f32> {
    let d = vec2<f32>(textureDimensions(src));
    var acc = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y += 2) {
        for (var x = -1; x <= 1; x += 2) {
            let c = textureSample(src, smp, v.uv + vec2<f32>(f32(x), f32(y)) * 2.0 / d).rgb;
            let k = clamp((luma(c) - 0.7) / 0.3, 0.0, 1.0);
            acc += c * k;
        }
    }
    return vec4<f32>(acc * 0.25, 1.0);
}

fn blur(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let d = 1.0 / vec2<f32>(textureDimensions(src));
    var w = array<f32, 5>(0.2270, 0.1945, 0.1216, 0.0541, 0.0162);
    var acc = textureSample(src, smp, uv).rgb * w[0];
    for (var i = 1; i < 5; i++) {
        let o = dir * d * f32(i) * 1.6;
        acc += textureSample(src, smp, uv + o).rgb * w[i];
        acc += textureSample(src, smp, uv - o).rgb * w[i];
    }
    return vec4<f32>(acc, 1.0);
}

@fragment
fn fs_blur_h(v: V) -> @location(0) vec4<f32> { return blur(v.uv, vec2<f32>(1.0, 0.0)); }

@fragment
fn fs_blur_v(v: V) -> @location(0) vec4<f32> { return blur(v.uv, vec2<f32>(0.0, 1.0)); }

fn aces(c: vec3<f32>) -> vec3<f32> {
    let x = c * 1.1;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(v: V) -> @location(0) vec4<f32> {
    var c = textureSample(src, smp, v.uv).rgb;
    // light sharpen (also hides the softness of a reduced render scale)
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let avg = (textureSample(src, smp, v.uv + vec2<f32>(t.x, 0.0)).rgb + textureSample(src, smp, v.uv - vec2<f32>(t.x, 0.0)).rgb + textureSample(src, smp, v.uv + vec2<f32>(0.0, t.y)).rgb + textureSample(src, smp, v.uv - vec2<f32>(0.0, t.y)).rgb) * 0.25;
    c = max(c + (c - avg) * 0.3, vec3<f32>(0.0));
    let b = textureSample(bloom, smp, v.uv).rgb;
    var col = c + b * 0.9;
    col = mix(col, aces(col), 0.7);
    // gentle vignette
    let q = v.uv - vec2<f32>(0.5);
    col *= 1.0 - dot(q, q) * 0.55;
    return vec4<f32>(col, 1.0);
}
