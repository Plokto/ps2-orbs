use crate::scenes::{
    create_blend_state, create_buffer, create_rasterizer, set_viewport, update_cbuffer,
    update_dynamic_slice, BlendMode, FrameInput, Scene, ShaderCompiler, FULLSCREEN_VS_HLSL,
};
use std::f32::consts::TAU;
use std::mem::size_of;
use windows::core::s;
use windows::Win32::Foundation::TRUE;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState, ID3D11DepthStencilView, ID3D11Device,
    ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
    ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DEPTH_STENCIL,
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
    D3D11_CLEAR_DEPTH, D3D11_COMPARISON_LESS_EQUAL, D3D11_COMPARISON_NEVER, D3D11_CULL_FRONT,
    D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_DEPTH_WRITE_MASK_ZERO,
    D3D11_FILL_SOLID, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC,
    D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE_ADDRESS_WRAP,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_R32G32B32A32_FLOAT,
    DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC,
};

const ROWS: usize = 14;
const COLS: usize = 9;
const VERTS_PER_TOWER: usize = 30;
const MAX_TOWER_VERTICES: usize = ROWS * COLS * VERTS_PER_TOWER;

const FOG_LAYERS: usize = 5;
const FOG_VERTS: usize = FOG_LAYERS * 6;

const LIGHT_COUNT: usize = 4;
const TRAIL_SEGMENTS: usize = 36;
// Per light: 36 trail quads (216 verts) + 1 halo quad (6 verts) + 1 core quad (6 verts)
const LIGHT_MAX_VERTS: usize = LIGHT_COUNT * (TRAIL_SEGMENTS + 2) * 6;

const CUBE_COUNT: usize = 5;
const CUBE_VERTS: usize = 36;

const REFRACT_W: u32 = 800;
const REFRACT_H: u32 = 450;

// Exact count -> C / D tables from retail PS2 OSDSYS (aap/osdbits/opening.c:3249)
const HIST_TABLE_C: [f32; 14] = [
    0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
];
const HIST_TABLE_D: [f32; 14] = [
    0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
];

// Exact 21x6 neighbourhood table tiling all 126 cells (opening.c:3257)
const HIST_CELLS: [[[u8; 2]; 6]; 21] = [
    [[0, 0], [0, 1], [0, 2], [0, 3], [1, 0], [1, 1]],
    [[1, 2], [2, 0], [2, 1], [2, 2], [3, 0], [4, 0]],
    [[4, 1], [5, 0], [5, 1], [6, 0], [6, 1], [6, 2]],
    [[7, 0], [7, 1], [8, 0], [8, 1], [8, 2], [8, 3]],
    [[9, 0], [9, 1], [10, 0], [10, 1], [10, 2], [11, 0]],
    [[12, 0], [12, 1], [13, 0], [13, 1], [13, 2], [13, 3]],
    [[1, 3], [2, 3], [3, 1], [3, 2], [3, 3], [4, 2]],
    [[5, 2], [5, 3], [5, 4], [6, 3], [7, 2], [7, 3]],
    [[9, 2], [9, 3], [9, 5], [10, 3], [10, 4], [10, 5]],
    [[11, 1], [11, 2], [11, 3], [11, 4], [12, 2], [12, 3]],
    [[0, 4], [1, 4], [2, 4], [2, 5], [2, 6], [3, 4]],
    [[3, 5], [3, 6], [4, 3], [4, 4], [4, 5], [4, 6]],
    [[5, 5], [5, 6], [6, 4], [6, 5], [7, 4], [7, 5]],
    [[8, 4], [8, 5], [8, 6], [9, 4], [9, 6], [9, 7]],
    [[12, 4], [13, 4], [13, 5], [13, 6], [13, 7], [13, 8]],
    [[0, 5], [0, 6], [1, 5], [1, 6], [1, 7], [2, 7]],
    [[0, 7], [0, 8], [1, 8], [2, 8], [3, 7], [3, 8]],
    [[4, 7], [4, 8], [5, 7], [5, 8], [6, 6], [6, 7]],
    [[6, 8], [7, 6], [7, 7], [7, 8], [8, 7], [8, 8]],
    [[9, 8], [10, 6], [10, 7], [10, 8], [11, 7], [11, 8]],
    [[11, 5], [11, 6], [12, 5], [12, 6], [12, 7], [12, 8]],
];

// Exact [14][9][3] source position table from retail OSDSYS (opening.c:3281)
#[rustfmt::skip]
const HIST_SRC_POS: [[[f32; 3]; 9]; 14] = [
    [[-13.7428,11.7512,-2.5366],[-13.7428,10.4512,-2.5366],[-13.7428,9.1512,-2.5366],[-13.1428,7.8512,-2.5366],[-13.7428,6.5512,-6.5517],[-13.7428,5.2512,-2.5366],[-13.7428,3.9512,-2.5366],[-13.7428,2.6512,-2.5366],[-13.7428,1.3512,-2.5366]],
    [[-12.4428,11.7512,-2.5366],[-12.4428,10.4512,-2.5366],[-12.4428,9.1512,-2.5366],[-12.1428,7.8512,-0.9330],[-12.4428,6.5512,-2.5366],[-12.4428,5.2512,-2.5366],[-12.4428,3.9512,-2.5366],[-12.4428,2.6512,-2.5366],[-12.4428,1.3512,-2.5366]],
    [[-11.1428,11.7512,-2.5366],[-11.1428,10.4512,-2.5366],[-11.1428,9.1512,-3.2422],[-11.1428,7.8512,-2.5366],[-11.1428,6.5512,-3.1781],[-11.1428,5.2512,-2.5366],[-11.1428,3.9512,-2.5366],[-11.1428,2.6512,-2.5366],[-11.1428,1.3512,-2.5366]],
    [[-9.8428,11.7512,-2.5366],[-9.8428,10.4512,-2.5366],[-9.8428,9.1512,-2.5366],[-9.8428,7.8512,-3.3705],[-9.8428,6.5512,-4.0119],[-9.8428,5.2512,-2.5366],[-9.8428,3.9512,-2.5366],[-9.8428,2.6512,-2.5366],[-9.8428,1.3512,-3.2422]],
    [[-8.5428,11.7512,-2.5366],[-8.5428,10.4512,-2.5366],[-8.5428,9.1512,-2.5366],[-8.5428,7.8512,-5.4054],[-8.5428,6.5512,-4.4491],[-8.5428,5.2512,-2.5366],[-8.5428,3.9512,-3.5629],[-8.5428,2.6512,-2.5366],[-8.5428,1.3512,-2.5366]],
    [[-7.2428,11.7512,-2.5366],[-7.2428,10.4512,-2.5366],[-7.2428,9.1512,-2.5366],[-7.2428,7.8512,-5.4097],[-7.2428,6.5512,-3.8195],[-7.2428,5.2512,-2.5366],[-7.2428,3.9512,-3.5629],[-7.2428,2.6512,-2.5366],[-7.2428,1.3512,-2.5366]],
    [[-5.9428,11.7512,-2.5366],[-5.9428,10.4512,-2.5366],[-5.9428,9.1512,-2.5366],[-5.9428,7.8512,-3.3705],[-5.9428,6.5512,-4.4491],[-5.9428,5.2512,-3.8195],[-5.9428,3.9512,-0.0309],[-5.9428,2.6512,-2.5366],[-5.9428,1.3512,-2.5366]],
    [[-4.6428,11.7512,-2.5366],[-4.6428,10.4512,-2.5366],[-4.6428,9.1512,-2.5366],[-4.6428,7.8512,-2.5366],[-4.6428,6.5512,-3.8195],[-4.6428,5.2512,-2.5366],[-4.6428,3.9512,-2.5366],[-4.6428,2.6512,-4.5251],[-4.6428,1.3512,-2.5366]],
    [[-3.3428,11.7512,-2.5366],[-3.3428,10.4512,-2.5366],[-3.3428,9.1512,-5.0383],[-3.3428,7.8512,-4.5892],[-3.3428,6.5512,-3.5629],[-3.3428,5.2512,-4.5766],[-3.3428,3.9512,-2.8536],[-3.3428,2.6512,-0.6190],[-3.3428,1.3512,-2.5366]],
    [[-2.0428,11.7512,-2.5366],[-2.0428,10.4512,-2.5366],[-2.0428,9.1512,-2.5366],[-2.0428,7.8512,-2.5366],[-2.0428,6.5512,-3.3654],[-2.0428,5.2512,-2.8799],[-2.0428,3.9512,-2.5366],[-2.0428,2.6512,-2.5366],[-2.0428,1.3512,-2.5366]],
    [[-0.7428,11.7512,-2.5366],[-0.7428,10.4512,-2.5366],[-0.7428,9.1512,0.7348],[-0.7428,7.8512,-2.5366],[-0.7428,6.5512,-2.5366],[-0.7428,5.2512,-2.8554],[-0.7428,3.9512,-4.2670],[-0.7428,2.6512,-2.5366],[-0.7428,1.3512,-2.5366]],
    [[0.5572,11.7512,-2.5366],[0.5572,10.4512,-2.5366],[0.5572,9.1512,-2.5366],[0.5572,7.8512,-3.0466],[0.5572,6.5512,-3.6204],[0.5572,5.2512,-2.8554],[0.5572,3.9512,-2.5366],[0.5572,2.6512,-2.5366],[0.5572,1.3512,-2.5366]],
    [[1.8572,11.7512,-2.5366],[1.8572,10.4512,-2.5366],[1.8572,9.1512,-2.5366],[1.8572,7.8512,-2.5366],[1.8572,6.5512,-2.5366],[1.8572,5.2512,-3.1741],[1.8572,3.9512,-2.5366],[1.8572,2.6512,-2.5366],[1.8572,1.3512,-2.5366]],
    [[3.1572,11.7512,-2.5366],[3.1572,10.4512,-2.5366],[3.1572,9.1512,-2.5366],[3.1572,7.8512,-2.5366],[3.1572,6.5512,-2.5366],[3.1572,5.2512,-2.5366],[3.1572,3.9512,-2.5366],[3.1572,2.6512,-2.5366],[3.1572,1.3512,-2.5366]],
];

const CUBE_SEEDS: [[f32; 3]; CUBE_COUNT] = [
    [3.5679, 0.5447, 2.5932],
    [-0.9042, -1.1173, 3.7952],
    [3.2639, -2.6491, 4.1075],
    [-3.7296, -2.3677, 4.3654],
    [-3.1017, 2.2409, 4.5429],
];

const LIGHT_COLORS: [[f32; 3]; LIGHT_COUNT] = [
    [0.25, 1.00, 0.12], // Green
    [1.00, 0.24, 0.56], // Magenta
    [1.00, 0.14, 0.10], // Red
    [0.54, 0.28, 1.00], // Violet
];

#[repr(C)]
#[derive(Clone, Copy)]
struct TowerVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    meta: [f32; 4], // [u_off, vertex_shade, face_type, seed_hash]
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FogVertex {
    pos: [f32; 3],
    world_xy: [f32; 2],
    layer_idx: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LightVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 3],
    mode: f32, // 0 = ribbon, 1 = soft halo, 2 = bright star core
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CubeVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TowerConsts {
    mvp: [[f32; 4]; 4],
    time_aspect: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CubeConsts {
    mvp: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    center_uv_params: [f32; 4], // [center_u, center_v, aspect, has_refract]
    time_pass: [f32; 4],        // [time, is_front, inv_refract_w, inv_refract_h]
}

const TOWER_VS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    float4 u_time_aspect;
};
struct VSIn {
    float3 pos  : POSITION;
    float2 uv   : TEXCOORD0;
    float4 meta : TEXCOORD1;
};
struct VSOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float4 meta  : TEXCOORD1;
    float  distv : TEXCOORD2;
};
VSOut main(VSIn input) {
    VSOut o;
    float4 clip = mul(float4(input.pos, 1.0), u_mvp);
    o.pos = clip;
    o.uv = input.uv;
    o.meta = input.meta;
    o.distv = saturate((clip.w - 115.0) / 145.0);
    return o;
}
"#;

const TOWER_PS_HLSL: &str = r#"
Texture2D<float> u_wall_tex : register(t0);
SamplerState u_wrap : register(s0);
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    float4 u_time_aspect;
};
struct PSIn {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float4 meta  : TEXCOORD1;
    float  distv : TEXCOORD2;
};

float pixel_hash(float2 p, uint salt) {
    uint2 ip = uint2(int2(floor(p)));
    uint h = ip.x * 374761393u + ip.y * 668265263u + salt * 1442695041u;
    h = (h ^ (h >> 13)) * 1274126177u;
    h = h ^ (h >> 16);
    return float(h & 0xFFFFu) * (1.0 / 65535.0);
}

float4 main(PSIn input) : SV_Target {
    float u_off = input.meta.x;
    float shade = input.meta.y;
    float is_cap = input.meta.z;
    float seed = input.meta.w;

    // Seamless weathered mineral / concrete grain matching PS2 TEXOWAL0 via pre-baked 128x128 LUT
    float2 base = input.uv + float2(seed * 7.13 + u_off * 1.7, seed * 3.71 + u_off * 1.1);
    float grain = u_wall_tex.Sample(u_wrap, base * 1.65);
    float tex_grain = 0.72 + 0.30 * grain;

    // Subtle directional contrast + preserve crisp stone grain even on hero bright center caps
    float2 edge = min(input.uv, 1.0 - input.uv);
    float bevel = 1.0 - smoothstep(0.0, 0.032, min(edge.x, edge.y));
    float cap_boost = is_cap * bevel * 0.035 * saturate(shade);

    float lit = pow(saturate(shade), 1.15) * 0.98;
    float3 col = float3(0.84, 0.89, 1.00) * (lit * tex_grain) + float3(0.14, 0.20, 0.34) * cap_boost;

    // Subtle deep-indigo atmospheric attenuation on distant tails
    col = lerp(col, col * float3(0.68, 0.74, 1.00), input.distv * 0.45);

    float dither = (pixel_hash(input.pos.xy, 7u) - 0.5) * (1.0 / 255.0);
    return float4(saturate(col + dither), 1.0);
}
"#;

const FOG_VS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    float4 u_time_aspect;
};
struct VSIn {
    float3 pos       : POSITION;
    float2 world_xy  : TEXCOORD0;
    float  layer_idx : TEXCOORD1;
};
struct VSOut {
    float4 pos       : SV_Position;
    float2 world_xy  : TEXCOORD0;
    nointerpolation float  layer_idx : TEXCOORD1;
};
VSOut main(VSIn input) {
    VSOut o;
    o.pos = mul(float4(input.pos, 1.0), u_mvp);
    o.world_xy = input.world_xy;
    o.layer_idx = input.layer_idx;
    return o;
}
"#;

const FOG_PS_HLSL: &str = r#"
Texture2D<float> u_fog_tex : register(t0);
SamplerState u_wrap : register(s0);
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    float4 u_time_aspect;
};
struct PSIn {
    float4 pos       : SV_Position;
    float2 world_xy  : TEXCOORD0;
    nointerpolation float  layer_idx : TEXCOORD1;
};

float pixel_hash(float2 p, uint salt) {
    uint2 ip = uint2(int2(floor(p)));
    uint h = ip.x * 374761393u + ip.y * 668265263u + salt * 1442695041u;
    h = (h ^ (h >> 13)) * 1274126177u;
    h = h ^ (h >> 16);
    return float(h & 0xFFFFu) * (1.0 / 65535.0);
}

float4 main(PSIn input) : SV_Target {
    float t = u_time_aspect.x;
    uint li_u = uint(round(input.layer_idx));
    float li = float(li_u); // 0 (deepest z=160) .. 4 (closest z=108)

    // InitFog centered radial mask around (-2.0, 0.0) fading smoothly to 0 by r=37.0
    float2 d = input.world_xy - float2(-2.0, 0.0);
    float r = length(d * float2(0.88, 1.05));
    float radial = smoothstep(37.0, 2.5, r);
    radial *= radial;
    if (radial <= 0.001) {
        return float4(0.0, 0.0, 0.0, 1.0);
    }

    // Per-layer rotated UV + domain-warped 2-tap billowing PS2 clouds from 128x128 FBM LUT
    float ang = li * 0.92 + t * (0.014 - li * 0.003);
    float cs = cos(ang);
    float sn = sin(ang);
    float2 rot_uv = float2(d.x * cs - d.y * sn, d.x * sn + d.y * cs) * (0.078 + li * 0.011);

    float2 drift = float2(
        t * (0.14 + li * 0.036) * (((li_u & 1u) == 0u) ? 1.0 : -0.85),
        t * (0.07 + li * 0.022)
    );
    float2 p = rot_uv + drift + float2(li * 13.7, li * 7.9);
    float warp = u_fog_tex.Sample(u_wrap, p * 0.18 + float2(0.31, -0.47));
    float cloud = u_fog_tex.Sample(u_wrap, p * 0.25 + float2(warp * 0.36, -warp * 0.31));

    // Sculpted wispy cloud billows + restrained chasm floor glow
    float wisp = smoothstep(0.33, 0.77, cloud);
    float depth_glow = lerp(0.14, 0.02, li / 4.0);
    float density = (depth_glow + 1.45 * wisp) * radial;

    // PS2 DrawFog palette: deep royal indigo below -> luminous cobalt-sapphire wisps above
    float3 deep_col = float3(0.045, 0.095, 0.32);
    float3 upper_col = float3(0.145, 0.260, 0.68);
    float3 layer_col = lerp(deep_col, upper_col, li / 4.0);

    float gain = lerp(0.50, 0.36, li / 4.0);
    float3 rgb = layer_col * (density * gain);

    // Uncorrelated TPDF integer hash dither breaks 8-bit contour rings with zero moire
    uint salt1 = li_u * 19u + 11u;
    uint salt2 = li_u * 37u + 53u;
    float tpdf = pixel_hash(input.pos.xy, salt1) + pixel_hash(input.pos.xy, salt2) - 1.0;
    float dither = tpdf * (1.25 / 255.0) * smoothstep(0.0, 0.015, rgb.b);
    return float4(max(rgb + dither, 0.0), 1.0);
}
"#;

const LIGHT_VS_HLSL: &str = r#"
struct VSIn {
    float2 pos   : POSITION;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
    float  mode  : TEXCOORD1;
};
struct VSOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
    float  mode  : TEXCOORD1;
};
VSOut main(VSIn input) {
    VSOut o;
    o.pos = float4(input.pos, 0.0, 1.0);
    o.uv = input.uv;
    o.color = input.color;
    o.mode = input.mode;
    return o;
}
"#;

const LIGHT_PS_HLSL: &str = r#"
struct PSIn {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
    float  mode  : TEXCOORD1;
};
float4 main(PSIn input) : SV_Target {
    if (input.mode < 0.5) {
        // Anti-aliased ribbon segment: uv.x in [0..1] longitudinal taper, uv.y in [-1..1] across width
        float cross_falloff = exp(-input.uv.y * input.uv.y * 4.2) * saturate(1.0 - input.uv.y * input.uv.y);
        return float4(input.color * (cross_falloff * input.uv.x), 1.0);
    } else if (input.mode < 1.5) {
        // Soft colored outer orb halo
        float r2 = dot(input.uv, input.uv);
        float d = saturate(1.0 - r2);
        float glow = pow(d, 2.6);
        return float4(input.color * glow, 1.0);
    } else {
        // Bright pinpoint star core
        float r2 = dot(input.uv, input.uv);
        float core = exp(-r2 * 6.5) * saturate(1.0 - r2);
        return float4(input.color * core, 1.0);
    }
}
"#;

const DOWNSAMPLE_PS_HLSL: &str = r#"
Texture2D<float4> u_tex : register(t0);
SamplerState u_samp : register(s0);
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    row_major float4x4 u_view;
    float4 u_center_uv_params;
    float4 u_time_pass; // .zw = inv_refract_w, inv_refract_h
};
struct PSIn {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};
float4 main(PSIn input) : SV_Target {
    float2 texel = u_time_pass.zw * 0.65;
    float3 c0 = u_tex.Sample(u_samp, input.uv + float2(-texel.x, -texel.y)).rgb;
    float3 c1 = u_tex.Sample(u_samp, input.uv + float2( texel.x, -texel.y)).rgb;
    float3 c2 = u_tex.Sample(u_samp, input.uv + float2(-texel.x,  texel.y)).rgb;
    float3 c3 = u_tex.Sample(u_samp, input.uv + float2( texel.x,  texel.y)).rgb;
    return float4((c0 + c1 + c2 + c3) * 0.25, 1.0);
}
"#;

const CUBE_VS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    row_major float4x4 u_view;
    float4 u_center_uv_params; // xy = cube center UV, z = aspect, w = has_refract
    float4 u_time_pass;        // x = time, y = is_front
};
struct VSIn {
    float3 pos    : POSITION;
    float3 normal : NORMAL;
    float2 uv     : TEXCOORD0;
};
struct VSOut {
    float4 pos         : SV_Position;
    float4 clip_pos    : TEXCOORD0;
    float3 view_normal : TEXCOORD1;
    float3 view_pos    : TEXCOORD2;
    float2 local_uv    : TEXCOORD3;
};
VSOut main(VSIn input) {
    VSOut o;
    float4 world = float4(input.pos, 1.0);
    float4 clip = mul(world, u_mvp);
    o.pos = clip;
    o.clip_pos = clip;
    o.view_normal = normalize(mul(float4(input.normal, 0.0), u_view).xyz);
    o.view_pos = mul(world, u_view).xyz;
    o.local_uv = input.uv;
    return o;
}
"#;

const CUBE_PS_HLSL: &str = r#"
Texture2D<float4> u_refract : register(t0);
SamplerState u_samp : register(s0);
cbuffer Params : register(b0) {
    row_major float4x4 u_mvp;
    row_major float4x4 u_view;
    float4 u_center_uv_params; // xy = cube center UV, z = aspect, w = has_refract
    float4 u_time_pass;        // x = time, y = is_front
};
struct PSIn {
    float4 pos         : SV_Position;
    float4 clip_pos    : TEXCOORD0;
    float3 view_normal : TEXCOORD1;
    float3 view_pos    : TEXCOORD2;
    float2 local_uv    : TEXCOORD3;
};

float pixel_hash(float2 p) {
    uint2 ip = uint2(int2(floor(p)));
    uint h = ip.x * 374761393u + ip.y * 668265263u;
    h = (h ^ (h >> 13)) * 1274126177u;
    h = h ^ (h >> 16);
    return float(h & 0xFFFFu) * (1.0 / 65535.0);
}

float4 main(PSIn input) : SV_Target {
    float2 ndc = input.clip_pos.xy / max(input.clip_pos.w, 1e-4);
    float2 screen_uv = float2(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    float2 center_uv = u_center_uv_params.xy;
    bool has_refract = u_center_uv_params.w > 0.5;
    float t = u_time_pass.x;
    bool is_front = u_time_pass.y > 0.5;

    float3 vn = normalize(input.view_normal);
    float3 vv = normalize(-input.view_pos);
    float fresnel_cos = abs(dot(vn, vv));
    float fresnel = pow(saturate(1.0 - fresnel_cos), 2.0);

    // Note: texture UV has +U right, +V down; view normal has +X right, +Y up
    float2 uv_n = float2(vn.x, -vn.y);

    // Subtle bevel highlight only near cube crystal edges
    float2 edge = min(input.local_uv, 1.0 - input.local_uv);
    float edge_glint = 1.0 - smoothstep(0.0, 0.028, min(edge.x, edge.y));

    // Scrolling env sheen inspired by BLPR / BLP / REF texture passes
    float sheen_band = sin((input.local_uv.x * 1.4 - input.local_uv.y * 1.1 + vn.x * 0.9) * 6.28318 + t * 0.55);
    float frost = 0.5 + 0.5 * sheen_band;

    if (!is_front) {
        // Sub-pass 5a: Interior Back Faces (refract background inside cube cavity)
        float2 back_uv = clamp(screen_uv + uv_n * 0.070, float2(0.002, 0.002), float2(0.998, 0.998));
        float3 bg = has_refract ? u_refract.Sample(u_samp, back_uv).rgb : float3(0.05, 0.07, 0.16);
        float3 interior = bg * float3(0.76, 0.82, 0.94);
        interior += float3(0.06, 0.09, 0.18) * (frost * 0.35 + fresnel * 0.45);
        interior += float3(0.09, 0.12, 0.22) * (edge_glint * 0.14);
        float alpha = has_refract ? 1.0 : saturate(0.45 + fresnel * 0.30);
        return float4(saturate(interior), alpha);
    } else {
        // Sub-pass 5b: Exterior Front Faces (PS2 center-lens -0.12 zoom + dispersion + Fresnel sheen)
        float2 base_uv = screen_uv + (screen_uv - center_uv) * (-0.12) - uv_n * 0.075;
        float2 disp = uv_n * 0.005;

        float3 refracted;
        if (has_refract) {
            float r = u_refract.Sample(u_samp, clamp(base_uv + disp, 0.002, 0.998)).r;
            float g = u_refract.Sample(u_samp, clamp(base_uv, 0.002, 0.998)).g;
            float b = u_refract.Sample(u_samp, clamp(base_uv - disp, 0.002, 0.998)).b;
            refracted = float3(r, g, b) * float3(0.88, 0.92, 1.00);
        } else {
            refracted = float3(0.14, 0.20, 0.42);
        }

        float3 env_streak = float3(0.28, 0.36, 0.58) * pow(frost, 3.0) * 0.34;
        float3 rim = float3(0.55, 0.66, 0.92) * fresnel;
        float3 edge_col = float3(0.52, 0.62, 0.88) * (edge_glint * 0.10);

        float3 front_col = refracted + env_streak + rim + edge_col;
        float alpha = saturate(0.32 + 0.54 * fresnel + pow(frost, 3.0) * 0.22 + edge_glint * 0.10);
        float dither = (pixel_hash(input.pos.xy) - 0.5) * (1.0 / 255.0);
        return float4(saturate(front_col + dither), alpha);
    }
}
"#;

struct TowerSpec {
    x: f32,
    y: f32,
    near_z: f32,
    far_z: f32,
    brightness: f32,
    u_off: f32,
    can_sway: bool,
    seed_hash: f32,
}

struct CameraFrame {
    view: [[f32; 4]; 4],
    mvp: [[f32; 4]; 4],
}

pub struct Ps2TowersScene {
    towers: Vec<TowerSpec>,
    tower_verts: Vec<TowerVertex>,
    fog_verts: Vec<FogVertex>,
    light_verts: Vec<LightVertex>,
    cube_verts: Vec<CubeVertex>,

    tower_vs: ID3D11VertexShader,
    tower_ps: ID3D11PixelShader,
    tower_il: ID3D11InputLayout,
    tower_vb: ID3D11Buffer,
    tower_cb: ID3D11Buffer,

    fog_vs: ID3D11VertexShader,
    fog_ps: ID3D11PixelShader,
    fog_il: ID3D11InputLayout,
    fog_vb: ID3D11Buffer,

    light_vs: ID3D11VertexShader,
    light_ps: ID3D11PixelShader,
    light_il: ID3D11InputLayout,
    light_vb: ID3D11Buffer,

    fullscreen_vs: ID3D11VertexShader,
    downsample_ps: ID3D11PixelShader,

    cube_vs: ID3D11VertexShader,
    cube_ps: ID3D11PixelShader,
    cube_il: ID3D11InputLayout,
    cube_vb: ID3D11Buffer,
    cube_cb: ID3D11Buffer,

    linear_clamp: ID3D11SamplerState,
    linear_wrap: ID3D11SamplerState,
    _wall_tex: ID3D11Texture2D,
    wall_srv: ID3D11ShaderResourceView,
    _fog_tex: ID3D11Texture2D,
    fog_srv: ID3D11ShaderResourceView,
    _refract_tex: ID3D11Texture2D,
    refract_rtv: ID3D11RenderTargetView,
    refract_srv: ID3D11ShaderResourceView,

    opaque_blend: ID3D11BlendState,
    alpha_blend: ID3D11BlendState,
    add_blend: ID3D11BlendState,

    rast_cull_back: ID3D11RasterizerState,
    rast_cull_front: ID3D11RasterizerState,
    rast_nocull: ID3D11RasterizerState,

    ds_write: ID3D11DepthStencilState,
    ds_readonly: ID3D11DepthStencilState,
    ds_disabled: ID3D11DepthStencilState,
    _depth_tex: ID3D11Texture2D,
    dsv: ID3D11DepthStencilView,
}

fn create_depth(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<(ID3D11Texture2D, ID3D11DepthStencilView), String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width.max(1),
        Height: height.max(1),
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_D16_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    unsafe {
        let mut tex: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .map_err(|e| format!("CreateTexture2D depth failed: {e}"))?;
        let tex = tex.ok_or("No depth texture")?;
        let mut dsv: Option<ID3D11DepthStencilView> = None;
        device
            .CreateDepthStencilView(&tex, None, Some(&mut dsv))
            .map_err(|e| format!("CreateDepthStencilView failed: {e}"))?;
        Ok((tex, dsv.ok_or("No DSV")?))
    }
}

fn create_refract_target(
    device: &ID3D11Device,
) -> Result<(ID3D11Texture2D, ID3D11RenderTargetView, ID3D11ShaderResourceView), String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: REFRACT_W,
        Height: REFRACT_H,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    unsafe {
        let mut tex: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .map_err(|e| format!("CreateTexture2D refract failed: {e}"))?;
        let tex = tex.ok_or("No refract texture")?;
        let mut rtv: Option<ID3D11RenderTargetView> = None;
        device
            .CreateRenderTargetView(&tex, None, Some(&mut rtv))
            .map_err(|e| format!("CreateRenderTargetView refract failed: {e}"))?;
        let mut srv: Option<ID3D11ShaderResourceView> = None;
        device
            .CreateShaderResourceView(&tex, None, Some(&mut srv))
            .map_err(|e| format!("CreateShaderResourceView refract failed: {e}"))?;
        Ok((tex, rtv.ok_or("No refract RTV")?, srv.ok_or("No refract SRV")?))
    }
}

fn create_r8_texture(
    device: &ID3D11Device,
    size: u32,
    data: &[u8],
) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView), String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: size,
        Height: size,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let sub = D3D11_SUBRESOURCE_DATA {
        pSysMem: data.as_ptr() as *const _,
        SysMemPitch: size,
        SysMemSlicePitch: 0,
    };
    unsafe {
        let mut tex: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&desc, Some(&sub), Some(&mut tex))
            .map_err(|e| format!("CreateTexture2D r8 failed: {e}"))?;
        let tex = tex.ok_or("No r8 texture")?;
        let mut srv: Option<ID3D11ShaderResourceView> = None;
        device
            .CreateShaderResourceView(&tex, None, Some(&mut srv))
            .map_err(|e| format!("CreateShaderResourceView r8 failed: {e}"))?;
        Ok((tex, srv.ok_or("No r8 SRV")?))
    }
}

fn periodic_noise(x: f32, y: f32, px: u32, py: u32, salt: u32) -> f32 {
    let hash = |ix: u32, iy: u32| -> f32 {
        let mut h = ix
            .wrapping_mul(374_761_393)
            .wrapping_add(iy.wrapping_mul(668_265_263))
            .wrapping_add(salt.wrapping_mul(1_442_695_041));
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        h ^= h >> 16;
        (h & 0xFFFF) as f32 / 65535.0
    };
    let gx = x * px as f32;
    let gy = y * py as f32;
    let ix = gx.floor() as i32;
    let iy = gy.floor() as i32;
    let fx = gx - ix as f32;
    let fy = gy - iy as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let x0 = ix.rem_euclid(px as i32) as u32;
    let x1 = (ix + 1).rem_euclid(px as i32) as u32;
    let y0 = iy.rem_euclid(py as i32) as u32;
    let y1 = (iy + 1).rem_euclid(py as i32) as u32;

    let a = hash(x0, y0);
    let b = hash(x1, y0);
    let c = hash(x0, y1);
    let d = hash(x1, y1);
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uy
}

struct NoiseLuts {
    wall_tex: ID3D11Texture2D,
    wall_srv: ID3D11ShaderResourceView,
    fog_tex: ID3D11Texture2D,
    fog_srv: ID3D11ShaderResourceView,
}

fn build_noise_luts(device: &ID3D11Device) -> Result<NoiseLuts, String> {
    let size = 128u32;
    let inv = 1.0 / size as f32;
    let mut wall_data = Vec::with_capacity((size * size) as usize);
    let mut fog_data = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        let y = (py as f32 + 0.5) * inv;
        for px in 0..size {
            let x = (px as f32 + 0.5) * inv;
            let wv = 0.46 * periodic_noise(x, y, 8, 24, 1)
                + 0.34 * periodic_noise(x, y, 24, 8, 2)
                + 0.20 * periodic_noise(x, y, 48, 48, 3);
            let fv = 0.54 * periodic_noise(x, y, 4, 4, 11)
                + 0.30 * periodic_noise(x, y, 8, 8, 12)
                + 0.16 * periodic_noise(x, y, 16, 16, 13);
            wall_data.push((wv.clamp(0.0, 1.0) * 255.0).round() as u8);
            fog_data.push((fv.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let (wall_tex, wall_srv) = create_r8_texture(device, size, &wall_data)?;
    let (fog_tex, fog_srv) = create_r8_texture(device, size, &fog_data)?;
    Ok(NoiseLuts {
        wall_tex,
        wall_srv,
        fog_tex,
        fog_srv,
    })
}

#[derive(Clone, Copy, Default)]
struct HistEntry {
    active: bool,
    title: u16,
    count: usize,
    mask: u8,
    own: u8,
    date: u16,
}

fn generate_towers() -> Vec<TowerSpec> {
    // 1. Exact HeightGrid (0x217e30) from retail PS2 OSDSYS (opening.c:3191)
    let sq = 5202.0f32.sqrt();
    let mut tower_grid = [[0.0f32; 20]; 20];
    for (i, row) in tower_grid.iter_mut().enumerate() {
        let gx = (i as f32 - 10.0) * 5.1 + 2.55;
        for (j, cell) in row.iter_mut().enumerate() {
            let gy = (j as f32 - 10.0) * 5.1 + 2.55;
            let d1 = 2.0 * ((-5.1 - gx).hypot(gy));
            let d2 = 4.0 * ((10.2 - gx).hypot(5.1 - gy));
            let mut v = (255.0 * (sq - d1) / sq).clamp(32.0, 255.0)
                + (255.0 * (sq - d2) / sq * 0.5).clamp(32.0, 255.0);
            v *= 0.85;
            let dither = (((((i + j) * i) / (j + 1)) % 11) as i32 - 5) as f32 * 10.0;
            v = (v - dither).clamp(32.0, 220.0);
            *cell = v;
        }
    }

    // 2. Exact SimulateBootHistory (opening.c:3410) with deterministic seed
    let mut rand_state: u32 = 0x2b99_2ddf;
    let mut tower_rand = || -> usize {
        rand_state = rand_state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((rand_state >> 16) & 0x7FFF) as usize
    };

    let ngames = 21usize;
    let nboots = 560usize;
    let mut sim_history = [HistEntry::default(); 21];

    for b in 0..nboots {
        let r1 = tower_rand() % ngames;
        let r2 = tower_rand() % ngames;
        let title = if tower_rand() % 3 != 0 {
            r1.min(r2)
        } else {
            r1
        } as u16;

        if let Some(e) = sim_history.iter_mut().find(|h| h.active && h.title == title) {
            e.date = b as u16;
            if (e.mask & 0x3F) == 0x3F {
                if e.count < 63 {
                    e.count += 1;
                } else {
                    e.own = 7;
                }
            } else {
                if e.count < 127 {
                    e.count += 1;
                }
                if e.count >= 14 && (e.count - 14) % 10 == 0 {
                    let mut bit = (tower_rand() % 6) as u8;
                    while ((e.mask >> bit) & 1) != 0 {
                        bit = (tower_rand() % 6) as u8;
                    }
                    e.own = bit;
                    e.mask |= 1 << bit;
                }
            }
        } else {
            let nempty = sim_history.iter().filter(|h| !h.active).count();
            let slot_idx = if nempty > 0 {
                let mut pick = tower_rand() % nempty;
                sim_history
                    .iter()
                    .position(|h| {
                        if !h.active {
                            if pick == 0 {
                                true
                            } else {
                                pick -= 1;
                                false
                            }
                        } else {
                            false
                        }
                    })
                    .unwrap_or(0)
            } else {
                let mut best = 0usize;
                for i in 1..21 {
                    if sim_history[i].count < sim_history[best].count
                        || (sim_history[i].count == sim_history[best].count
                            && sim_history[i].date < sim_history[best].date)
                    {
                        best = i;
                    }
                }
                best
            };
            let own = (tower_rand() % 6) as u8;
            sim_history[slot_idx] = HistEntry {
                active: true,
                title,
                count: 1,
                mask: 1 << own,
                own,
                date: b as u16,
            };
        }
    }

    // 3. Parse boot history into [14][9] grid (opening.c:3529)
    let mut flags = [[false; COLS]; ROWS];
    let mut tower_c = [[0.0f32; COLS]; ROWS];
    let mut tower_d = [[0.0f32; COLS]; ROWS];

    for (i, h) in sim_history.iter().enumerate() {
        if !h.active {
            continue;
        }
        let tidx = if h.count < 14 {
            h.count
        } else {
            (h.count - 14) % 10 + 4
        };
        for (t, cell_rc) in HIST_CELLS[i].iter().enumerate() {
            let r = cell_rc[0] as usize;
            let c = cell_rc[1] as usize;
            if t == h.own as usize {
                flags[r][c] = true;
                tower_c[r][c] = HIST_TABLE_C[tidx];
                tower_d[r][c] = HIST_TABLE_D[tidx];
            } else if ((h.mask >> t) & 1) != 0 {
                flags[r][c] = true;
                tower_c[r][c] = 1.0;
                tower_d[r][c] = 1.0;
            }
        }
    }

    // 4. GenerateTowerField + Step 5 conversion (opening.c:3568, 3850)
    let mut out = Vec::with_capacity(ROWS * COLS);
    for r in 0..ROWS {
        for c in 0..COLS {
            if !flags[r][c] {
                continue;
            }
            let src = HIST_SRC_POS[r][c];
            let x = (src[0] + 4.8) * 4.0;
            let y = -(src[1] - 6.5) * 4.0;
            let base_z = (src[2] + 4.0) * 12.0 + 150.0;

            let tc = tower_c[r][c];
            let td = tower_d[r][c];
            let half_len = (td * 30.0).max(3.0);
            let center_z = base_z + tc * 30.0 - half_len;

            let bint = if tc >= 1.0 {
                0
            } else {
                ((1.0 - tc) * 128.0) as i32
            };
            let gridval = tower_grid[r + 3][c + 6] / 220.0;
            let brightness = (gridval
                * if bint != 0 {
                    bint as f32 / 128.0
                } else {
                    half_len / 30.0
                })
            .clamp(0.05, 1.0);

            let a0 = ((r + c + 9) * (r + 8) / (c + 7)) + ((r + c + 9) * (r + 7) / (c + 9));
            let u_off = a0 as f32 / 256.0;
            let seed_hash = ((a0 * 37 + r * 13 + c * 7) & 0xFF) as f32 / 255.0;

            out.push(TowerSpec {
                x,
                y,
                near_z: center_z - half_len,
                far_z: center_z + half_len,
                brightness,
                u_off,
                can_sway: tc < 1.0,
                seed_hash,
            });
        }
    }
    // Front-to-back sort lets hardware Early-Z skip occluded rear tower shafts
    out.sort_by(|a, b| a.near_z.total_cmp(&b.near_z));
    out
}

fn build_fog_quads() -> Vec<FogVertex> {
    // 5 3D depth-tested fog planes with tightened XY bounds matching r<=37 radial cutoff
    let z_layers = [160.0f32, 136.0, 126.0, 116.0, 108.0];
    let mut out = Vec::with_capacity(FOG_VERTS);
    for (li, &z) in z_layers.iter().enumerate() {
        let l = li as f32;
        let p0 = FogVertex { pos: [-41.0, 34.0, z], world_xy: [-41.0, 34.0], layer_idx: l };
        let p1 = FogVertex { pos: [37.0, 34.0, z], world_xy: [37.0, 34.0], layer_idx: l };
        let p2 = FogVertex { pos: [37.0, -34.0, z], world_xy: [37.0, -34.0], layer_idx: l };
        let p3 = FogVertex { pos: [-41.0, -34.0, z], world_xy: [-41.0, -34.0], layer_idx: l };
        out.extend_from_slice(&[p0, p1, p2, p0, p2, p3]);
    }
    out
}

fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for col in 0..4 {
            m[row][col] = a[row][0] * b[0][col]
                + a[row][1] * b[1][col]
                + a[row][2] * b[2][col]
                + a[row][3] * b[3][col];
        }
    }
    m
}

fn mul_point(p: [f32; 3], m: &[[f32; 4]; 4]) -> [f32; 4] {
    [
        p[0] * m[0][0] + p[1] * m[1][0] + p[2] * m[2][0] + m[3][0],
        p[0] * m[0][1] + p[1] * m[1][1] + p[2] * m[2][1] + m[3][1],
        p[0] * m[0][2] + p[1] * m[1][2] + p[2] * m[2][2] + m[3][2],
        p[0] * m[0][3] + p[1] * m[1][3] + p[2] * m[2][3] + m[3][3],
    ]
}

fn build_camera(t: f32, aspect: f32) -> CameraFrame {
    let eye = [
        2.2 * (t * 0.068).sin(),
        1.5 * (t * 0.057).cos(),
        22.0 + 6.0 * (t * 0.085).sin(),
    ];
    let target = [-1.6, 0.0, 150.0];
    let roll = -0.14 + 0.12 * (t * 0.052).sin();
    let up_raw = [roll.sin(), roll.cos(), 0.0];

    let norm3 = |v: [f32; 3]| -> [f32; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross3 = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot3 = |a: [f32; 3], b: [f32; 3]| -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };

    let f = norm3([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let r = norm3(cross3(up_raw, f));
    let u = cross3(f, r);

    let fovy = 21.4f32.to_radians();
    let ys = 1.0 / (fovy * 0.5).tan();
    let xs = ys / aspect.max(0.1);
    let near = 5.0f32;
    let far = 360.0f32;
    let zf = far / (far - near);
    let zn = -near * far / (far - near);

    let view = [
        [r[0], u[0], f[0], 0.0],
        [r[1], u[1], f[1], 0.0],
        [r[2], u[2], f[2], 0.0],
        [-dot3(r, eye), -dot3(u, eye), -dot3(f, eye), 1.0],
    ];
    let proj = [
        [xs, 0.0, 0.0, 0.0],
        [0.0, ys, 0.0, 0.0],
        [0.0, 0.0, zf, 1.0],
        [0.0, 0.0, zn, 0.0],
    ];

    CameraFrame {
        view,
        mvp: mat4_mul(&view, &proj),
    }
}

fn rotate_euler_xyz(p: [f32; 3], ax: f32, ay: f32, az: f32) -> [f32; 3] {
    let (sx, cx) = ax.sin_cos();
    let (sy, cy) = ay.sin_cos();
    let (sz, cz) = az.sin_cos();
    // X
    let y1 = p[1] * cx - p[2] * sx;
    let z1 = p[1] * sx + p[2] * cx;
    // Y
    let x2 = p[0] * cy + z1 * sy;
    let z2 = -p[0] * sy + z1 * cy;
    // Z
    let x3 = x2 * cz - y1 * sz;
    let y3 = x2 * sz + y1 * cz;
    [x3, y3, z2]
}

impl Ps2TowersScene {
    pub fn new(device: &ID3D11Device, width: u32, height: u32) -> Result<Self, String> {
        let compiler = ShaderCompiler::new()?;
        let (tvs_bytes, tower_vs) = compiler.create_vs(device, TOWER_VS_HLSL)?;
        let tower_ps = compiler.create_ps(device, TOWER_PS_HLSL)?;
        let (fvs_bytes, fog_vs) = compiler.create_vs(device, FOG_VS_HLSL)?;
        let fog_ps = compiler.create_ps(device, FOG_PS_HLSL)?;
        let (lvs_bytes, light_vs) = compiler.create_vs(device, LIGHT_VS_HLSL)?;
        let light_ps = compiler.create_ps(device, LIGHT_PS_HLSL)?;
        let (_, fullscreen_vs) = compiler.create_vs(device, FULLSCREEN_VS_HLSL)?;
        let downsample_ps = compiler.create_ps(device, DOWNSAMPLE_PS_HLSL)?;
        let (cvs_bytes, cube_vs) = compiler.create_vs(device, CUBE_VS_HLSL)?;
        let cube_ps = compiler.create_ps(device, CUBE_PS_HLSL)?;
        drop(compiler);

        let tower_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 12,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 1,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 20,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let fog_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 12,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 1,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 20,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let light_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 8,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("COLOR"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 16,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 1,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 28,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let cube_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("NORMAL"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 12,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 24,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let create_il = |layout: &[D3D11_INPUT_ELEMENT_DESC], bytes: &[u8]| -> Result<ID3D11InputLayout, String> {
            unsafe {
                let mut il: Option<ID3D11InputLayout> = None;
                device
                    .CreateInputLayout(layout, bytes, Some(&mut il))
                    .map_err(|e| format!("CreateInputLayout failed: {e}"))?;
                il.ok_or_else(|| "No input layout".to_string())
            }
        };

        let tower_il = create_il(&tower_layout, &tvs_bytes)?;
        let fog_il = create_il(&fog_layout, &fvs_bytes)?;
        let light_il = create_il(&light_layout, &lvs_bytes)?;
        let cube_il = create_il(&cube_layout, &cvs_bytes)?;

        let tower_vb = create_buffer(
            device,
            (MAX_TOWER_VERTICES * size_of::<TowerVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;
        let tower_cb = create_buffer(
            device,
            size_of::<TowerConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;

        let fog_verts = build_fog_quads();
        let fog_vb = create_buffer(
            device,
            (fog_verts.len() * size_of::<FogVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            false,
            Some(fog_verts.as_ptr() as *const _),
        )?;

        let light_vb = create_buffer(
            device,
            (LIGHT_MAX_VERTS * size_of::<LightVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;

        let cube_vb = create_buffer(
            device,
            (CUBE_VERTS * size_of::<CubeVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;
        let cube_cb = create_buffer(
            device,
            size_of::<CubeConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;

        let mut samp_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let linear_clamp = unsafe {
            let mut s_out: Option<ID3D11SamplerState> = None;
            device
                .CreateSamplerState(&samp_desc, Some(&mut s_out))
                .map_err(|e| format!("CreateSamplerState clamp: {e}"))?;
            s_out.ok_or("No clamp sampler")?
        };
        samp_desc.AddressU = D3D11_TEXTURE_ADDRESS_WRAP;
        samp_desc.AddressV = D3D11_TEXTURE_ADDRESS_WRAP;
        samp_desc.AddressW = D3D11_TEXTURE_ADDRESS_WRAP;
        let linear_wrap = unsafe {
            let mut s_out: Option<ID3D11SamplerState> = None;
            device
                .CreateSamplerState(&samp_desc, Some(&mut s_out))
                .map_err(|e| format!("CreateSamplerState wrap: {e}"))?;
            s_out.ok_or("No wrap sampler")?
        };

        let NoiseLuts {
            wall_tex,
            wall_srv,
            fog_tex,
            fog_srv,
        } = build_noise_luts(device)?;
        let (refract_tex, refract_rtv, refract_srv) = create_refract_target(device)?;

        let opaque_blend = create_blend_state(device, BlendMode::Opaque)?;
        let alpha_blend = create_blend_state(device, BlendMode::Alpha)?;
        let add_blend = create_blend_state(device, BlendMode::Additive)?;
        let rast_cull_back = create_rasterizer(device, true, true)?;
        let rast_nocull = create_rasterizer(device, false, false)?;

        let cull_front_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_FRONT,
            FrontCounterClockwise: false.into(),
            DepthClipEnable: TRUE,
            ..Default::default()
        };
        let rast_cull_front = unsafe {
            let mut rs: Option<ID3D11RasterizerState> = None;
            device
                .CreateRasterizerState(&cull_front_desc, Some(&mut rs))
                .map_err(|e| format!("rast_cull_front: {e}"))?;
            rs.ok_or("No rast_cull_front")?
        };

        let ds_write_desc = D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: TRUE,
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
            DepthFunc: D3D11_COMPARISON_LESS_EQUAL,
            ..Default::default()
        };
        let ds_readonly_desc = D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: TRUE,
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
            DepthFunc: D3D11_COMPARISON_LESS_EQUAL,
            ..Default::default()
        };
        let ds_off_desc = D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: false.into(),
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
            ..Default::default()
        };
        let create_ds = |d: &D3D11_DEPTH_STENCIL_DESC| -> Result<ID3D11DepthStencilState, String> {
            unsafe {
                let mut s_out: Option<ID3D11DepthStencilState> = None;
                device
                    .CreateDepthStencilState(d, Some(&mut s_out))
                    .map_err(|e| format!("CreateDepthStencilState: {e}"))?;
                s_out.ok_or_else(|| "No DS".to_string())
            }
        };
        let ds_write = create_ds(&ds_write_desc)?;
        let ds_readonly = create_ds(&ds_readonly_desc)?;
        let ds_disabled = create_ds(&ds_off_desc)?;

        let (depth_tex, dsv) = create_depth(device, width, height)?;

        Ok(Self {
            towers: generate_towers(),
            tower_verts: Vec::with_capacity(MAX_TOWER_VERTICES),
            fog_verts,
            light_verts: Vec::with_capacity(LIGHT_MAX_VERTS),
            cube_verts: Vec::with_capacity(CUBE_VERTS),
            tower_vs,
            tower_ps,
            tower_il,
            tower_vb,
            tower_cb,
            fog_vs,
            fog_ps,
            fog_il,
            fog_vb,
            light_vs,
            light_ps,
            light_il,
            light_vb,
            fullscreen_vs,
            downsample_ps,
            cube_vs,
            cube_ps,
            cube_il,
            cube_vb,
            cube_cb,
            linear_clamp,
            linear_wrap,
            _wall_tex: wall_tex,
            wall_srv,
            _fog_tex: fog_tex,
            fog_srv,
            _refract_tex: refract_tex,
            refract_rtv,
            refract_srv,
            opaque_blend,
            alpha_blend,
            add_blend,
            rast_cull_back,
            rast_cull_front,
            rast_nocull,
            ds_write,
            ds_readonly,
            ds_disabled,
            _depth_tex: depth_tex,
            dsv,
        })
    }

    fn build_tower_geometry(&mut self, t: f32) {
        self.tower_verts.clear();
        let sway_angle = (t * 0.45).sin() * 6.0f32.to_radians();
        let hw = 2.05f32;

        for tw in &self.towers {
            let ang = if tw.can_sway { sway_angle } else { 0.0 };
            let (sa, ca) = ang.sin_cos();
            let rot = |dx: f32, dy: f32| -> (f32, f32) {
                (tw.x + dx * ca - dy * sa, tw.y + dx * sa + dy * ca)
            };

            let (bl_x, bl_y) = rot(-hw, -hw);
            let (tl_x, tl_y) = rot(-hw, hw);
            let (tr_x, tr_y) = rot(hw, hw);
            let (br_x, br_y) = rot(hw, -hw);

            let z0 = tw.near_z;
            let z1 = tw.far_z;
            let b = tw.brightness;
            let uo = tw.u_off;
            let sh = tw.seed_hash;

            let push_quad = |verts: &mut Vec<TowerVertex>,
                             p0: [f32; 3],
                             p1: [f32; 3],
                             p2: [f32; 3],
                             p3: [f32; 3],
                             s0: f32,
                             s1: f32,
                             face_type: f32| {
                verts.extend_from_slice(&[
                    TowerVertex { pos: p0, uv: [0.0, 0.0], meta: [uo, s0, face_type, sh] },
                    TowerVertex { pos: p1, uv: [1.0, 0.0], meta: [uo, s0, face_type, sh] },
                    TowerVertex { pos: p2, uv: [1.0, 1.0], meta: [uo, s1, face_type, sh] },
                    TowerVertex { pos: p0, uv: [0.0, 0.0], meta: [uo, s0, face_type, sh] },
                    TowerVertex { pos: p2, uv: [1.0, 1.0], meta: [uo, s1, face_type, sh] },
                    TowerVertex { pos: p3, uv: [0.0, 1.0], meta: [uo, s1, face_type, sh] },
                ]);
            };

            // Near cap (-Z, CW outward: TL -> TR -> BR -> BL)
            push_quad(
                &mut self.tower_verts,
                [tl_x, tl_y, z0],
                [tr_x, tr_y, z0],
                [br_x, br_y, z0],
                [bl_x, bl_y, z0],
                b * 1.02,
                b * 1.02,
                1.0,
            );
            // Top (+Y, CW outward: far-left -> far-right -> near-right -> near-left)
            push_quad(
                &mut self.tower_verts,
                [tl_x, tl_y, z1],
                [tr_x, tr_y, z1],
                [tr_x, tr_y, z0],
                [tl_x, tl_y, z0],
                0.0,
                b * 0.92,
                0.0,
            );
            // Bottom (-Y, CW outward: near-left -> near-right -> far-right -> far-left)
            push_quad(
                &mut self.tower_verts,
                [bl_x, bl_y, z0],
                [br_x, br_y, z0],
                [br_x, br_y, z1],
                [bl_x, bl_y, z1],
                b * 0.40,
                0.0,
                0.0,
            );
            // Right (+X, CW outward: near-top -> far-top -> far-bot -> near-bot)
            push_quad(
                &mut self.tower_verts,
                [tr_x, tr_y, z0],
                [tr_x, tr_y, z1],
                [br_x, br_y, z1],
                [br_x, br_y, z0],
                b * 0.50,
                0.0,
                0.0,
            );
            // Left (-X, CW outward: far-top -> near-top -> near-bot -> far-bot)
            push_quad(
                &mut self.tower_verts,
                [tl_x, tl_y, z1],
                [tl_x, tl_y, z0],
                [bl_x, bl_y, z0],
                [bl_x, bl_y, z1],
                b * 0.80,
                0.0,
                0.0,
            );
        }
    }

    fn build_lights(&mut self, t: f32, aspect: f32, mvp: &[[f32; 4]; 4]) {
        self.light_verts.clear();
        let project_ndc = |tau: f32, l: usize| -> Option<[f32; 2]> {
            let lf = l as f32;
            let frame = tau * 60.0 + 4002.0;
            let c = ((frame + lf * 17.0) * 0.01 * (lf + 10.0) * 0.1).cos();
            let s = ((frame + lf * 15.0) * 0.005 * (lf + 10.0) * 0.1).sin();
            let wp = [
                (10.0 - lf) * c * 1.55,
                -(3.0 + lf) * s * 1.55,
                c * 12.0 + 90.0,
            ];
            let clip = mul_point(wp, mvp);
            if clip[3] <= 0.1 {
                return None;
            }
            Some([clip[0] / clip[3], clip[1] / clip[3]])
        };

        let inv_aspect = 1.0 / aspect.max(0.1);
        for (l, &col) in LIGHT_COLORS.iter().enumerate() {
            let mut pts = [[0.0f32; 2]; TRAIL_SEGMENTS + 1];
            let mut valid = true;
            for (k, slot) in pts.iter_mut().enumerate() {
                if let Some(p) = project_ndc(t - k as f32 * 0.078, l) {
                    *slot = p;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }

            // Curved fading light trail ribbons
            for k in 0..TRAIL_SEGMENTS {
                let p_curr = pts[k];
                let p_next = pts[k + 1];
                let u_curr = 1.0 - (k as f32 / TRAIL_SEGMENTS as f32);
                let u_next = 1.0 - ((k + 1) as f32 / TRAIL_SEGMENTS as f32);
                let u_curr = u_curr * u_curr;
                let u_next = u_next * u_next;

                let dx = (p_curr[0] - p_next[0]) * aspect;
                let dy = p_curr[1] - p_next[1];
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let nx = -dy / len;
                let ny = dx / len;

                let w_curr = 0.0045 * u_curr + 0.0012 * (1.0 - u_curr);
                let w_next = 0.0045 * u_next + 0.0012 * (1.0 - u_next);

                let off_c = [nx * w_curr * inv_aspect, ny * w_curr];
                let off_n = [nx * w_next * inv_aspect, ny * w_next];

                let c_curr = [col[0] * 0.88, col[1] * 0.88, col[2] * 0.88];

                let a = [p_curr[0] + off_c[0], p_curr[1] + off_c[1]];
                let b = [p_curr[0] - off_c[0], p_curr[1] - off_c[1]];
                let c = [p_next[0] - off_n[0], p_next[1] - off_n[1]];
                let d = [p_next[0] + off_n[0], p_next[1] + off_n[1]];

                self.light_verts.extend_from_slice(&[
                    LightVertex { pos: a, uv: [u_curr, 1.0], color: c_curr, mode: 0.0 },
                    LightVertex { pos: d, uv: [u_next, 1.0], color: c_curr, mode: 0.0 },
                    LightVertex { pos: c, uv: [u_next, -1.0], color: c_curr, mode: 0.0 },
                    LightVertex { pos: a, uv: [u_curr, 1.0], color: c_curr, mode: 0.0 },
                    LightVertex { pos: c, uv: [u_next, -1.0], color: c_curr, mode: 0.0 },
                    LightVertex { pos: b, uv: [u_curr, -1.0], color: c_curr, mode: 0.0 },
                ]);
            }

            // Head orb: colored halo + bright pinpoint white core
            let head = pts[0];
            let hr_y = 0.030f32;
            let hr_x = hr_y * inv_aspect;
            let halo_col = [col[0] * 0.85, col[1] * 0.85, col[2] * 0.85];
            self.light_verts.extend_from_slice(&[
                LightVertex { pos: [head[0] - hr_x, head[1] + hr_y], uv: [-1.0, 1.0], color: halo_col, mode: 1.0 },
                LightVertex { pos: [head[0] + hr_x, head[1] + hr_y], uv: [1.0, 1.0], color: halo_col, mode: 1.0 },
                LightVertex { pos: [head[0] + hr_x, head[1] - hr_y], uv: [1.0, -1.0], color: halo_col, mode: 1.0 },
                LightVertex { pos: [head[0] - hr_x, head[1] + hr_y], uv: [-1.0, 1.0], color: halo_col, mode: 1.0 },
                LightVertex { pos: [head[0] + hr_x, head[1] - hr_y], uv: [1.0, -1.0], color: halo_col, mode: 1.0 },
                LightVertex { pos: [head[0] - hr_x, head[1] - hr_y], uv: [-1.0, -1.0], color: halo_col, mode: 1.0 },
            ]);

            let cr_y = 0.011f32;
            let cr_x = cr_y * inv_aspect;
            let core_col = [1.0, 1.0, 1.0];
            self.light_verts.extend_from_slice(&[
                LightVertex { pos: [head[0] - cr_x, head[1] + cr_y], uv: [-1.0, 1.0], color: core_col, mode: 2.0 },
                LightVertex { pos: [head[0] + cr_x, head[1] + cr_y], uv: [1.0, 1.0], color: core_col, mode: 2.0 },
                LightVertex { pos: [head[0] + cr_x, head[1] - cr_y], uv: [1.0, -1.0], color: core_col, mode: 2.0 },
                LightVertex { pos: [head[0] - cr_x, head[1] + cr_y], uv: [-1.0, 1.0], color: core_col, mode: 2.0 },
                LightVertex { pos: [head[0] + cr_x, head[1] - cr_y], uv: [1.0, -1.0], color: core_col, mode: 2.0 },
                LightVertex { pos: [head[0] - cr_x, head[1] - cr_y], uv: [-1.0, -1.0], color: core_col, mode: 2.0 },
            ]);
        }
    }

    fn build_single_cube(&mut self, i: usize, t: f32) -> [f32; 3] {
        self.cube_verts.clear();
        let seed = CUBE_SEEDS[i];
        let d = if i == 2 { 0.9 } else { (i as f32 - 2.0) * 0.8 };
        let anchor = [
            seed[0] * 3.5,
            -seed[1] * 3.5,
            seed[2] * -15.0 + 150.0,
        ];

        let frames = t * 60.0;
        let v0 = d * ((i % 3) as f32) * 3.7 + 0.2856;
        let ax = (v0 + (0.0031 / d) * frames).rem_euclid(TAU);
        let ay = (v0 + (d * 0.0022) * frames).rem_euclid(TAU);
        let az = (v0 + (d / 1000.0 + 0.0013) * frames).rem_euclid(TAU);

        let h = 1.48f32;
        let local_corners = [
            [-h, -h, -h], // 0: L B N
            [h, -h, -h],  // 1: R B N
            [-h, h, -h],  // 2: L T N
            [h, h, -h],   // 3: R T N
            [-h, -h, h],  // 4: L B F
            [h, -h, h],   // 5: R B F
            [-h, h, h],   // 6: L T F
            [h, h, h],    // 7: R T F
        ];
        let mut world_corners = [[0.0f32; 3]; 8];
        for (idx, &lc) in local_corners.iter().enumerate() {
            let rc = rotate_euler_xyz(lc, ax, ay, az);
            world_corners[idx] = [rc[0] + anchor[0], rc[1] + anchor[1], rc[2] + anchor[2]];
        }

        // 6 cube faces in strict CW outward winding (TL -> TR -> BR -> BL)
        let faces: [([usize; 4], [f32; 3]); 6] = [
            ([2, 3, 1, 0], [0.0, 0.0, -1.0]), // Front (-Z)
            ([7, 6, 4, 5], [0.0, 0.0, 1.0]),  // Back (+Z)
            ([6, 7, 3, 2], [0.0, 1.0, 0.0]),  // Top (+Y)
            ([0, 1, 5, 4], [0.0, -1.0, 0.0]), // Bottom (-Y)
            ([3, 7, 5, 1], [1.0, 0.0, 0.0]),  // Right (+X)
            ([6, 2, 0, 4], [-1.0, 0.0, 0.0]), // Left (-X)
        ];

        for &(vis, ln) in &faces {
            let n = rotate_euler_xyz(ln, ax, ay, az);
            let p0 = world_corners[vis[0]];
            let p1 = world_corners[vis[1]];
            let p2 = world_corners[vis[2]];
            let p3 = world_corners[vis[3]];
            self.cube_verts.extend_from_slice(&[
                CubeVertex { pos: p0, normal: n, uv: [0.0, 0.0] },
                CubeVertex { pos: p1, normal: n, uv: [1.0, 0.0] },
                CubeVertex { pos: p2, normal: n, uv: [1.0, 1.0] },
                CubeVertex { pos: p0, normal: n, uv: [0.0, 0.0] },
                CubeVertex { pos: p2, normal: n, uv: [1.0, 1.0] },
                CubeVertex { pos: p3, normal: n, uv: [0.0, 1.0] },
            ]);
        }

        anchor
    }
}

impl Scene for Ps2TowersScene {
    fn resize(&mut self, device: &ID3D11Device, width: u32, height: u32) -> Result<(), String> {
        let (tex, dsv) = create_depth(device, width, height)?;
        self._depth_tex = tex;
        self.dsv = dsv;
        Ok(())
    }

    fn render(
        &mut self,
        context: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        backbuf_srv: Option<&ID3D11ShaderResourceView>,
        input: &FrameInput,
    ) {
        let t = input.elapsed;
        let aspect = (input.width.max(1) as f32) / (input.height.max(1) as f32);
        let cam = build_camera(t, aspect);

        self.build_tower_geometry(t);
        self.build_lights(t, aspect, &cam.mvp);

        let t_consts = TowerConsts {
            mvp: cam.mvp,
            time_aspect: [t, aspect, 0.0, 0.0],
        };
        update_cbuffer(context, &self.tower_cb, &t_consts);
        update_dynamic_slice(context, &self.tower_vb, &self.tower_verts);
        update_dynamic_slice(context, &self.light_vb, &self.light_verts);

        unsafe {
            let offset = 0u32;
            let clear_black = [0.0f32, 0.0, 0.0, 1.0];
            let rtv_slice = [Some(rtv.clone())];
            let t_cbs = [Some(self.tower_cb.clone())];
            let no_srv: [Option<ID3D11ShaderResourceView>; 1] = [None];

            set_viewport(context, input.width, input.height);
            context.ClearRenderTargetView(rtv, &clear_black);
            context.ClearDepthStencilView(&self.dsv, D3D11_CLEAR_DEPTH.0, 1.0, 0);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            // Pass 1: 3D monolith towers (front-to-back Early-Z, pre-baked wall grain LUT)
            let wrap_samps = [Some(self.linear_wrap.clone())];
            let wall_srvs = [Some(self.wall_srv.clone())];
            let fog_srvs = [Some(self.fog_srv.clone())];

            context.OMSetRenderTargets(Some(&rtv_slice), &self.dsv);
            context.OMSetBlendState(&self.opaque_blend, None, 0xFFFF_FFFF);
            context.OMSetDepthStencilState(&self.ds_write, 0);
            context.RSSetState(&self.rast_cull_back);
            context.IASetInputLayout(&self.tower_il);
            context.VSSetShader(&self.tower_vs, None);
            context.PSSetShader(&self.tower_ps, None);
            context.VSSetConstantBuffers(0, Some(&t_cbs));
            context.PSSetConstantBuffers(0, Some(&t_cbs));
            context.PSSetSamplers(0, Some(&wrap_samps));
            context.PSSetShaderResources(0, Some(&wall_srvs));
            let t_stride = size_of::<TowerVertex>() as u32;
            let t_vbs = [Some(self.tower_vb.clone())];
            context.IASetVertexBuffers(0, 1, Some(t_vbs.as_ptr()), Some(&t_stride), Some(&offset));
            context.Draw(self.tower_verts.len() as u32, 0);

            // Pass 2: 5-layer 3D depth-tested cobalt-blue volumetric fog (2-tap LUT, read D16)
            context.OMSetBlendState(&self.add_blend, None, 0xFFFF_FFFF);
            context.OMSetDepthStencilState(&self.ds_readonly, 0);
            context.RSSetState(&self.rast_nocull);
            context.IASetInputLayout(&self.fog_il);
            context.VSSetShader(&self.fog_vs, None);
            context.PSSetShader(&self.fog_ps, None);
            context.PSSetShaderResources(0, Some(&fog_srvs));
            let f_stride = size_of::<FogVertex>() as u32;
            let f_vbs = [Some(self.fog_vb.clone())];
            context.IASetVertexBuffers(0, 1, Some(f_vbs.as_ptr()), Some(&f_stride), Some(&offset));
            context.Draw(self.fog_verts.len() as u32, 0);
            context.PSSetShaderResources(0, Some(&no_srv));

            // Pass 3: 4 orbiting light orbs + curved fading trails
            if !self.light_verts.is_empty() {
                context.OMSetRenderTargets(Some(&rtv_slice), None);
                context.OMSetDepthStencilState(&self.ds_disabled, 0);
                context.IASetInputLayout(&self.light_il);
                context.VSSetShader(&self.light_vs, None);
                context.PSSetShader(&self.light_ps, None);
                let l_stride = size_of::<LightVertex>() as u32;
                let l_vbs = [Some(self.light_vb.clone())];
                context.IASetVertexBuffers(0, 1, Some(l_vbs.as_ptr()), Some(&l_stride), Some(&offset));
                context.Draw(self.light_verts.len() as u32, 0);
            }

            // Pass 4: Fixed 800x450 softened scene capture for refractive glass cubes
            let has_refract = if let Some(src_srv) = backbuf_srv {
                context.OMSetRenderTargets(None, None);
                let ref_rtv = [Some(self.refract_rtv.clone())];
                context.OMSetRenderTargets(Some(&ref_rtv), None);
                set_viewport(context, REFRACT_W, REFRACT_H);
                context.OMSetBlendState(&self.opaque_blend, None, 0xFFFF_FFFF);
                context.OMSetDepthStencilState(&self.ds_disabled, 0);
                context.RSSetState(&self.rast_nocull);
                context.IASetInputLayout(None);
                context.VSSetShader(&self.fullscreen_vs, None);
                context.PSSetShader(&self.downsample_ps, None);
                let ds_consts = CubeConsts {
                    mvp: cam.mvp,
                    view: cam.view,
                    center_uv_params: [0.5, 0.5, aspect, 1.0],
                    time_pass: [t, 0.0, 1.0 / REFRACT_W as f32, 1.0 / REFRACT_H as f32],
                };
                update_cbuffer(context, &self.cube_cb, &ds_consts);
                let c_cbs = [Some(self.cube_cb.clone())];
                context.PSSetConstantBuffers(0, Some(&c_cbs));
                let samps = [Some(self.linear_clamp.clone())];
                context.PSSetSamplers(0, Some(&samps));
                let srvs = [Some(src_srv.clone())];
                context.PSSetShaderResources(0, Some(&srvs));
                context.Draw(3, 0);
                context.PSSetShaderResources(0, Some(&no_srv));
                context.OMSetRenderTargets(None, None);
                true
            } else {
                false
            };

            // Pass 5: 5 tumbling 2-depth refractive glass cubes (sorted back-to-front by seed Z)
            context.OMSetRenderTargets(Some(&rtv_slice), None);
            set_viewport(context, input.width, input.height);
            context.OMSetDepthStencilState(&self.ds_disabled, 0);
            context.IASetInputLayout(&self.cube_il);
            context.VSSetShader(&self.cube_vs, None);
            context.PSSetShader(&self.cube_ps, None);
            let c_cbs = [Some(self.cube_cb.clone())];
            context.VSSetConstantBuffers(0, Some(&c_cbs));
            context.PSSetConstantBuffers(0, Some(&c_cbs));
            let samps = [Some(self.linear_clamp.clone())];
            context.PSSetSamplers(0, Some(&samps));
            let ref_srvs = [Some(self.refract_srv.clone())];
            context.PSSetShaderResources(0, Some(&ref_srvs));

            let c_stride = size_of::<CubeVertex>() as u32;
            let c_vbs = [Some(self.cube_vb.clone())];
            context.IASetVertexBuffers(0, 1, Some(c_vbs.as_ptr()), Some(&c_stride), Some(&offset));

            let has_ref_f = if has_refract { 1.0 } else { 0.0 };
            let order = [0usize, 1, 2, 3, 4]; // Descending Z: 111.1, 93.1, 88.4, 84.5, 81.9
            for &ci in &order {
                let anchor = self.build_single_cube(ci, t);
                let cclip = mul_point(anchor, &cam.mvp);
                let cw = cclip[3].max(1e-4);
                let cu = (cclip[0] / cw) * 0.5 + 0.5;
                let cv = -(cclip[1] / cw) * 0.5 + 0.5;

                update_dynamic_slice(context, &self.cube_vb, &self.cube_verts);

                // Sub-pass 5a: Interior back faces (Opaque erase with interior refraction)
                let back_consts = CubeConsts {
                    mvp: cam.mvp,
                    view: cam.view,
                    center_uv_params: [cu, cv, aspect, has_ref_f],
                    time_pass: [t, 0.0, 1.0 / REFRACT_W as f32, 1.0 / REFRACT_H as f32],
                };
                update_cbuffer(context, &self.cube_cb, &back_consts);
                if has_refract {
                    context.OMSetBlendState(&self.opaque_blend, None, 0xFFFF_FFFF);
                } else {
                    context.OMSetBlendState(&self.alpha_blend, None, 0xFFFF_FFFF);
                }
                context.RSSetState(&self.rast_cull_front);
                context.Draw(CUBE_VERTS as u32, 0);

                // Sub-pass 5b: Exterior front faces (Alpha-composited center-lens refraction + Fresnel)
                let front_consts = CubeConsts {
                    mvp: cam.mvp,
                    view: cam.view,
                    center_uv_params: [cu, cv, aspect, has_ref_f],
                    time_pass: [t, 1.0, 1.0 / REFRACT_W as f32, 1.0 / REFRACT_H as f32],
                };
                update_cbuffer(context, &self.cube_cb, &front_consts);
                context.OMSetBlendState(&self.alpha_blend, None, 0xFFFF_FFFF);
                context.RSSetState(&self.rast_cull_back);
                context.Draw(CUBE_VERTS as u32, 0);
            }

            context.PSSetShaderResources(0, Some(&no_srv));
        }
    }
}
