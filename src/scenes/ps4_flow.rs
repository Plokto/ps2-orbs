use crate::scenes::{
    create_blend_state, create_buffer, create_rasterizer, create_vs_ps, set_viewport,
    update_cbuffer, update_dynamic_slice, BlendMode, FrameInput, Scene, FULLSCREEN_VS_HLSL,
};
use std::mem::size_of;
use windows::core::s;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout,
    ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11ShaderResourceView,
    ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_INDEX_BUFFER,
    D3D11_BIND_VERTEX_BUFFER, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
};

const GRID_W: usize = 80;
const GRID_H: usize = 40;
const MESH_VERTS: usize = GRID_W * GRID_H;
const MESH_INDICES: usize = (GRID_W - 1) * (GRID_H - 1) * 6;
const BOKEH_COUNT: usize = 56;
const BOKEH_VERTS: usize = BOKEH_COUNT * 6;

#[repr(C)]
#[derive(Clone, Copy)]
struct FlowVertex {
    uv: [f32; 2], // normalized grid [-1..1] x [-1..1]
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BokehVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FlowConsts {
    time: f32,
    aspect: f32,
    layer: f32,
    _pad: f32,
}

const BG_PS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    float u_time;
    float u_aspect;
    float u_layer;
    float _pad;
};
struct PSIn {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};
float4 main(PSIn input) : SV_Target {
    // Rich PS4 midnight-to-royal blue diagonal backdrop
    float grad = saturate(input.uv.y * 0.72 + (1.0 - input.uv.x) * 0.28);
    float3 top_c = float3(0.012, 0.045, 0.165);
    float3 mid_c = float3(0.032, 0.125, 0.365);
    float3 bot_c = float3(0.008, 0.025, 0.095);
    float3 col = (grad < 0.55)
        ? lerp(top_c, mid_c, grad / 0.55)
        : lerp(mid_c, bot_c, (grad - 0.55) / 0.45);
    float2 p = input.uv - float2(0.42 + 0.06 * sin(u_time * 0.12), 0.45);
    col += float3(0.02, 0.07, 0.22) * exp(-dot(p, p) * 2.8);
    float n = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * (1.0 / 255.0);
    return float4(max(col, 0.0), 1.0);
}
"#;

// 3D Perlin cnoise ported directly from alexkorzin/PlayStationBG planeVertex.glsl
const PLANE_VS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    float u_time;
    float u_aspect;
    float u_layer;
    float _pad;
};
struct VSIn {
    float2 uv : TEXCOORD0;
};
struct VSOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float  crest : TEXCOORD1;
};

float4 permute(float4 x) { return fmod(((x * 34.0) + 1.0) * x, 289.0); }
float4 taylorInvSqrt(float4 r) { return 1.79284291400159 - 0.85373472095314 * r; }
float3 fade(float3 t) { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }

float cnoise(float3 P) {
    float3 Pi0 = floor(P);
    float3 Pi1 = Pi0 + 1.0;
    Pi0 = fmod(Pi0, 289.0);
    Pi1 = fmod(Pi1, 289.0);
    float3 Pf0 = frac(P);
    float3 Pf1 = Pf0 - 1.0;
    float4 ix = float4(Pi0.x, Pi1.x, Pi0.x, Pi1.x);
    float4 iy = float4(Pi0.yy, Pi1.yy);
    float4 iz0 = Pi0.zzzz;
    float4 iz1 = Pi1.zzzz;
    float4 ixy = permute(permute(ix) + iy);
    float4 ixy0 = permute(ixy + iz0);
    float4 ixy1 = permute(ixy + iz1);
    float4 gx0 = ixy0 / 7.0;
    float4 gy0 = frac(floor(gx0) / 7.0) - 0.5;
    gx0 = frac(gx0);
    float4 gz0 = 0.5 - abs(gx0) - abs(gy0);
    float4 sz0 = step(gz0, 0.0);
    gx0 -= sz0 * (step(0.0, gx0) - 0.5);
    gy0 -= sz0 * (step(0.0, gy0) - 0.5);
    float4 gx1 = ixy1 / 7.0;
    float4 gy1 = frac(floor(gx1) / 7.0) - 0.5;
    gx1 = frac(gx1);
    float4 gz1 = 0.5 - abs(gx1) - abs(gy1);
    float4 sz1 = step(gz1, 0.0);
    gx1 -= sz1 * (step(0.0, gx1) - 0.5);
    gy1 -= sz1 * (step(0.0, gy1) - 0.5);
    float3 g000 = float3(gx0.x, gy0.x, gz0.x);
    float3 g100 = float3(gx0.y, gy0.y, gz0.y);
    float3 g010 = float3(gx0.z, gy0.z, gz0.z);
    float3 g110 = float3(gx0.w, gy0.w, gz0.w);
    float3 g001 = float3(gx1.x, gy1.x, gz1.x);
    float3 g101 = float3(gx1.y, gy1.y, gz1.y);
    float3 g011 = float3(gx1.z, gy1.z, gz1.z);
    float3 g111 = float3(gx1.w, gy1.w, gz1.w);
    float4 norm0 = taylorInvSqrt(float4(dot(g000, g000), dot(g010, g010), dot(g100, g100), dot(g110, g110)));
    g000 *= norm0.x; g010 *= norm0.y; g100 *= norm0.z; g110 *= norm0.w;
    float4 norm1 = taylorInvSqrt(float4(dot(g001, g001), dot(g011, g011), dot(g101, g101), dot(g111, g111)));
    g001 *= norm1.x; g011 *= norm1.y; g101 *= norm1.z; g111 *= norm1.w;
    float n000 = dot(g000, Pf0);
    float n100 = dot(g100, float3(Pf1.x, Pf0.yz));
    float n010 = dot(g010, float3(Pf0.x, Pf1.y, Pf0.z));
    float n110 = dot(g110, float3(Pf1.xy, Pf0.z));
    float n001 = dot(g001, float3(Pf0.xy, Pf1.z));
    float n101 = dot(g101, float3(Pf1.x, Pf0.y, Pf1.z));
    float n011 = dot(g011, float3(Pf0.x, Pf1.yz));
    float n111 = dot(g111, Pf1);
    float3 fade_xyz = fade(Pf0);
    float4 n_z = lerp(float4(n000, n100, n010, n110), float4(n001, n101, n011, n111), fade_xyz.z);
    float2 n_yz = lerp(n_z.xy, n_z.zw, fade_xyz.y);
    return 2.2 * lerp(n_yz.x, n_yz.y, fade_xyz.x);
}

VSOut main(VSIn input) {
    VSOut o;
    float2 g = input.uv; // [-1, 1]
    float t = u_time * 0.14 + u_layer * 4.5;
    float3 sample_p = float3(
        g.x * 1.15 + t * 0.55,
        g.y * 0.72 + u_layer * 1.9,
        t * 0.34
    );
    float h = cnoise(sample_p);
    float h2 = cnoise(sample_p * 1.5 + float3(2.1, -1.3, 0.7)) * 0.38;
    float disp = h * 0.18 + h2 * 0.07;

    // Gentle diagonal silk ribbon flow across screen without mesh self-folding
    float tilt = -0.14 + u_layer * 0.10;
    float x = g.x * 1.25;
    float y = (g.y * 0.64 + disp + g.x * tilt) + (u_layer - 0.5) * 0.16;

    o.pos = float4(x, y, 0.0, 1.0);
    o.uv = g;
    o.crest = saturate(0.48 + (h + h2) * 0.85);
    return o;
}
"#;

const PLANE_PS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    float u_time;
    float u_aspect;
    float u_layer;
    float _pad;
};
struct PSIn {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float  crest : TEXCOORD1;
};
float4 main(PSIn input) : SV_Target {
    // Soft ribbon falloff across top & bottom edges
    float edge_y = smoothstep(1.0, 0.12, abs(input.uv.y));
    float sheen = pow(saturate(input.crest), 1.8);
    float3 deep_blue = float3(0.05, 0.22, 0.72);
    float3 bright_cyan = float3(0.45, 0.80, 1.00);
    float3 col = lerp(deep_blue, bright_cyan, sheen);
    float alpha = edge_y * (0.14 + 0.34 * sheen);
    return float4(col, alpha);
}
"#;

const BOKEH_VS_HLSL: &str = r#"
struct VSIn {
    float2 pos   : POSITION;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
};
struct VSOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
};
VSOut main(VSIn input) {
    VSOut o;
    o.pos = float4(input.pos, 0.0, 1.0);
    o.uv = input.uv;
    o.color = input.color;
    return o;
}
"#;

const BOKEH_PS_HLSL: &str = r#"
struct PSIn {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float3 color : COLOR0;
};
float4 main(PSIn input) : SV_Target {
    float r2 = dot(input.uv, input.uv);
    float d = saturate(1.0 - r2);
    float glow = (d > 0.0) ? pow(d, 2.2) : 0.0;
    return float4(input.color * glow, 1.0);
}
"#;

struct BokehSeed {
    x0: f32,
    y0: f32,
    vx: f32,
    vy: f32,
    radius: f32,
    bright: f32,
    phase: f32,
}

pub struct Ps4FlowScene {
    bg_vs: ID3D11VertexShader,
    bg_ps: ID3D11PixelShader,
    plane_vs: ID3D11VertexShader,
    plane_ps: ID3D11PixelShader,
    plane_il: ID3D11InputLayout,
    plane_vb: ID3D11Buffer,
    plane_ib: ID3D11Buffer,
    bokeh_vs: ID3D11VertexShader,
    bokeh_ps: ID3D11PixelShader,
    bokeh_il: ID3D11InputLayout,
    bokeh_vb: ID3D11Buffer,
    cb: ID3D11Buffer,
    opaque_blend: ID3D11BlendState,
    alpha_blend: ID3D11BlendState,
    add_blend: ID3D11BlendState,
    rast_state: ID3D11RasterizerState,
    seeds: Vec<BokehSeed>,
    bokeh_verts: Vec<BokehVertex>,
}

impl Ps4FlowScene {
    pub fn new(device: &ID3D11Device) -> Result<Self, String> {
        let (_, bg_vs, bg_ps) = create_vs_ps(device, FULLSCREEN_VS_HLSL, BG_PS_HLSL)?;
        let (pvs_bytes, plane_vs, plane_ps) = create_vs_ps(device, PLANE_VS_HLSL, PLANE_PS_HLSL)?;
        let (bvs_bytes, bokeh_vs, bokeh_ps) = create_vs_ps(device, BOKEH_VS_HLSL, BOKEH_PS_HLSL)?;

        let plane_layout = [D3D11_INPUT_ELEMENT_DESC {
            SemanticName: s!("TEXCOORD"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        }];
        let bokeh_layout = [
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
        ];

        let (plane_il, bokeh_il) = unsafe {
            let mut pil: Option<ID3D11InputLayout> = None;
            device
                .CreateInputLayout(&plane_layout, &pvs_bytes, Some(&mut pil))
                .map_err(|e| format!("plane_il: {e}"))?;
            let mut bil: Option<ID3D11InputLayout> = None;
            device
                .CreateInputLayout(&bokeh_layout, &bvs_bytes, Some(&mut bil))
                .map_err(|e| format!("bokeh_il: {e}"))?;
            (pil.ok_or("No pil")?, bil.ok_or("No bil")?)
        };

        let mut verts = Vec::with_capacity(MESH_VERTS);
        for y in 0..GRID_H {
            let v = (y as f32 / (GRID_H - 1) as f32) * 2.0 - 1.0;
            for x in 0..GRID_W {
                let u = (x as f32 / (GRID_W - 1) as f32) * 2.0 - 1.0;
                verts.push(FlowVertex { uv: [u, v] });
            }
        }
        let mut indices = Vec::with_capacity(MESH_INDICES);
        for y in 0..GRID_H - 1 {
            for x in 0..GRID_W - 1 {
                let i0 = (y * GRID_W + x) as u16;
                let i1 = i0 + 1;
                let i2 = i0 + GRID_W as u16;
                let i3 = i2 + 1;
                indices.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
            }
        }

        let plane_vb = create_buffer(
            device,
            (verts.len() * size_of::<FlowVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            false,
            Some(verts.as_ptr() as *const _),
        )?;
        let plane_ib = create_buffer(
            device,
            (indices.len() * size_of::<u16>()) as u32,
            D3D11_BIND_INDEX_BUFFER.0 as u32,
            false,
            Some(indices.as_ptr() as *const _),
        )?;
        let bokeh_vb = create_buffer(
            device,
            (BOKEH_VERTS * size_of::<BokehVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;
        let cb = create_buffer(
            device,
            size_of::<FlowConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;

        let mut seed = 0x517cc1b7u32;
        let mut rng = || -> f32 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 16) & 0xFFFF) as f32 / 65535.0
        };
        let mut seeds = Vec::with_capacity(BOKEH_COUNT);
        for _ in 0..BOKEH_COUNT {
            seeds.push(BokehSeed {
                x0: rng() * 2.4 - 1.2,
                y0: rng() * 2.0 - 1.0,
                vx: (rng() - 0.5) * 0.04,
                vy: 0.015 + rng() * 0.035,
                radius: 0.012 + rng().powi(2) * 0.065,
                bright: 0.08 + rng() * 0.22,
                phase: rng() * std::f32::consts::TAU,
            });
        }

        Ok(Self {
            bg_vs,
            bg_ps,
            plane_vs,
            plane_ps,
            plane_il,
            plane_vb,
            plane_ib,
            bokeh_vs,
            bokeh_ps,
            bokeh_il,
            bokeh_vb,
            cb,
            opaque_blend: create_blend_state(device, BlendMode::Opaque)?,
            alpha_blend: create_blend_state(device, BlendMode::Alpha)?,
            add_blend: create_blend_state(device, BlendMode::Additive)?,
            rast_state: create_rasterizer(device, false, false)?,
            seeds,
            bokeh_verts: Vec::with_capacity(BOKEH_VERTS),
        })
    }
}

impl Scene for Ps4FlowScene {
    fn resize(&mut self, _device: &ID3D11Device, _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    fn render(
        &mut self,
        context: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        _backbuf_srv: Option<&ID3D11ShaderResourceView>,
        input: &FrameInput,
    ) {
        let t = if input.is_capture { 12.0 } else { input.elapsed };
        let aspect = (input.width.max(1) as f32) / (input.height.max(1) as f32);
        let inv_aspect = 1.0 / aspect;

        self.bokeh_verts.clear();
        for s in &self.seeds {
            let cx = ((s.x0 + s.vx * t + 1.2).rem_euclid(2.4)) - 1.2;
            let cy = ((s.y0 + s.vy * t + 1.1).rem_euclid(2.2)) - 1.1;
            let pulse = 0.65 + 0.35 * (t * 0.75 + s.phase).sin();
            let b = s.bright * pulse;
            let col = [0.28 * b, 0.68 * b, 1.0 * b];
            let rx = s.radius;
            let ry = s.radius;
            let x0 = (cx * aspect - rx) * inv_aspect;
            let x1 = (cx * aspect + rx) * inv_aspect;
            let y0 = cy - ry;
            let y1 = cy + ry;

            self.bokeh_verts.extend_from_slice(&[
                BokehVertex { pos: [x0, y0], uv: [-1.0, -1.0], color: col },
                BokehVertex { pos: [x1, y0], uv: [1.0, -1.0], color: col },
                BokehVertex { pos: [x1, y1], uv: [1.0, 1.0], color: col },
                BokehVertex { pos: [x0, y0], uv: [-1.0, -1.0], color: col },
                BokehVertex { pos: [x1, y1], uv: [1.0, 1.0], color: col },
                BokehVertex { pos: [x0, y1], uv: [-1.0, 1.0], color: col },
            ]);
        }
        update_dynamic_slice(context, &self.bokeh_vb, &self.bokeh_verts);

        unsafe {
            let rtv_slice = [Some(rtv.clone())];
            context.OMSetRenderTargets(Some(&rtv_slice), None);
            set_viewport(context, input.width, input.height);
            context.RSSetState(&self.rast_state);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            // Pass 1: Midnight-to-royal blue backdrop
            let bg_c = FlowConsts { time: t, aspect, layer: 0.0, _pad: 0.0 };
            update_cbuffer(context, &self.cb, &bg_c);
            context.OMSetBlendState(&self.opaque_blend, None, 0xFFFF_FFFF);
            context.IASetInputLayout(None);
            context.VSSetShader(&self.bg_vs, None);
            context.PSSetShader(&self.bg_ps, None);
            let cbs = [Some(self.cb.clone())];
            context.PSSetConstantBuffers(0, Some(&cbs));
            context.Draw(3, 0);

            // Pass 2: Dual Perlin cnoise flowing silk ribbons
            context.OMSetBlendState(&self.alpha_blend, None, 0xFFFF_FFFF);
            context.IASetInputLayout(&self.plane_il);
            context.VSSetShader(&self.plane_vs, None);
            context.PSSetShader(&self.plane_ps, None);
            let stride = size_of::<FlowVertex>() as u32;
            let offset = 0u32;
            let vbs = [Some(self.plane_vb.clone())];
            context.IASetVertexBuffers(0, 1, Some(vbs.as_ptr()), Some(&stride), Some(&offset));
            context.IASetIndexBuffer(&self.plane_ib, DXGI_FORMAT_R16_UINT, 0);

            for layer_idx in 0..2 {
                let lc = FlowConsts { time: t, aspect, layer: layer_idx as f32, _pad: 0.0 };
                update_cbuffer(context, &self.cb, &lc);
                context.VSSetConstantBuffers(0, Some(&cbs));
                context.PSSetConstantBuffers(0, Some(&cbs));
                context.DrawIndexed(MESH_INDICES as u32, 0, 0);
            }

            // Pass 3: Soft bokeh dust particles
            context.OMSetBlendState(&self.add_blend, None, 0xFFFF_FFFF);
            context.IASetInputLayout(&self.bokeh_il);
            context.VSSetShader(&self.bokeh_vs, None);
            context.PSSetShader(&self.bokeh_ps, None);
            let b_stride = size_of::<BokehVertex>() as u32;
            let b_vbs = [Some(self.bokeh_vb.clone())];
            context.IASetVertexBuffers(0, 1, Some(b_vbs.as_ptr()), Some(&b_stride), Some(&offset));
            context.Draw(self.bokeh_verts.len() as u32, 0);
        }
    }
}
