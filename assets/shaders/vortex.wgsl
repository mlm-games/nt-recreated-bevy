// Portal vortex (nt-rewrite objects SpiralCont/Spiral + scrDrawSpiral.gml)
// rendered as ONE procedural quad. Each wisp is a transformed sample of the
// real sprSpiral.png art; growth/alpha/lightning follow the GameMaker laws:
//   grow += 0.0002; xscale += grow; grow = (grow+1)*(1+0.0005*xscale)-1
//   -> closed form xscale(t) = 0.4*(cosh(sqrt(0.0005)*t)-1), t in 30Hz ticks
//   white pass + black(alpha 0.8 - xscale)  ~= brightness ramp 0.2 -> 1.0
//   destroy when xscale > 2.5               -> t ~= 118 ticks (~3.9 s)
//   lightning when 0 < lanim < 6            -> sprPortalLightning frame floor
//   depth = image_angle                     -> OLDEST wisps composite on top
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Plain vec4 array — no struct wrapper, so the host-side [[f32;4]; 128]
// uniform and the WGSL side share byte-identical layout (stride 16).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> wisps: array<vec4<f32>, 128>;
// (tick_now, lightning_enabled, bg_r, bg_g) then (bg_b, _, _, _)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> glob_a: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> glob_b: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var spiral_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var spiral_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var bolt_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var bolt_smp: sampler;

const N: u32 = 128u;
const SPIRAL_FRAMES: f32 = 2.0;
const BOLT_FRAMES: f32 = 6.0;
const K: f32 = 0.0223606798; // sqrt(0.0005)

fn wisp_scale(age: f32) -> f32 {
    return 0.4 * (cosh(K * age) - 1.0);
}

// Deterministic per-slot pseudo-random in [0,1) — stands in for the GML
// random() calls at Spiral/Create_0 (lanim start, langle) so the GPU never
// needs extra per-wisp state.
fn slot_rand(i: u32, salt: f32) -> f32 {
    let x = sin(f32(i) * 127.1 + salt * 311.7) * 43758.5453;
    return x - floor(x);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Quad spans the NT GUI surface; convert uv -> GameMaker coords (y-down).
    let gui = vec2<f32>(mesh.uv.x * 320.0, (1.0 - mesh.uv.y) * 240.0);
    let tick_now = glob_a.x;
    let lightning_on = glob_a.y;
    var acc = vec3<f32>(glob_a.z, glob_a.w, glob_b.x);

    // Oldest-on-top: walk slots newest -> oldest, src-over compositing.
    for (var j: u32 = 0u; j < N; j = j + 1u) {
        let i = (N - 1u) - j;
        let d = wisps[i];
        if (d.z < 0.0) { continue; } // empty slot
        let age = tick_now - d.z;
        if (age < 0.0) { continue; }
        let s = wisp_scale(age);
        if (s > 2.5) { continue; } // upstream destroy gate

        let alpha = clamp(0.2 + s, 0.0, 1.0);
        let half_ext = 32.0 * s * 10.0; // draw_sprite_ext(xs*10); sprite 64 px
        var rel = gui - d.xy;
        let c = cos(d.w);
        let sn = sin(d.w);
        rel = vec2<f32>(c * rel.x + sn * rel.y, -sn * rel.x + c * rel.y);
        let suv = rel / half_ext * 0.5 + vec2<f32>(0.5, 0.5);
        if (all((suv > vec2<f32>(0.0)) & (suv < vec2<f32>(1.0)))) {
            let tex = textureSampleLevel(
                spiral_tex, spiral_smp,
                vec2<f32>(suv.x / SPIRAL_FRAMES, suv.y), 0.0);
            acc = mix(acc, tex.rgb, tex.a * alpha);
        }

        // Lightning pass (scrDrawSpiral): only outside the Menu screen, while
        // this wisp's lanim clock crosses (0, 6). Native xscale, angle+langle.
        if (lightning_on > 0.5) {
            let seed = slot_rand(i, 1.0);
            let lanim = -300.0 * seed + 0.35 * age;
            if (lanim > 0.0 && lanim < 6.0 && s > 0.05) {
                let frame = clamp(floor(lanim), 0.0, BOLT_FRAMES - 1.0);
                let langle = slot_rand(i, 2.0) * 6.2831853;
                let lc = cos(d.w + langle);
                let ls = sin(d.w + langle);
                var lrel = gui - d.xy;
                lrel = vec2<f32>(lc * lrel.x + ls * lrel.y, -ls * lrel.x + lc * lrel.y);
                // bolt sprite 176x176, origin (180,88) -> offset origin-centre
                let lbolt_half = vec2<f32>(88.0, 88.0) * s;
                let buv = (lrel + vec2<f32>(180.0 - 88.0, 88.0 - 88.0) * s) / (lbolt_half * 2.0) + vec2<f32>(0.5, 0.5);
                if (all((buv > vec2<f32>(0.0)) & (buv < vec2<f32>(1.0)))) {
                    let btex = textureSampleLevel(
                        bolt_tex, bolt_smp,
                        vec2<f32>((frame + buv.x) / BOLT_FRAMES, buv.y), 0.0);
                    acc = mix(acc, btex.rgb, btex.a * 0.85);
                }
            }
        }
    }
    return vec4<f32>(acc, 1.0);
}
