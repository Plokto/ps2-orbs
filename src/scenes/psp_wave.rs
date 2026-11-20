use crate::scenes::{
    create_blend_state, create_buffer, create_rasterizer, create_vs_ps, set_viewport,
    update_cbuffer, BlendMode, FrameInput, Scene, FULLSCREEN_VS_HLSL,
};
use std::mem::size_of;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11ShaderResourceView, ID3D11VertexShader,
    D3D11_BIND_CONSTANT_BUFFER,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct PspConsts {
    time: f32,
    aspect: f32,
    _pad: [f32; 2],
}

const PS_HLSL: &str = r#"
cbuffer Params : register(b0) {
    float u_time;
    float u_aspect;
    float2 _pad;
};

struct PSIn {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};

float3 calc_sine(
    float2 uv,
    float speed,
    float frequency,
    float amplitude,
    float phase_shift,
    float vertical_offset,
    float3 base_color,
    float line_width,
    float sharpness,
    bool invert_falloff
) {
    float angle = u_time * speed * frequency * -1.0 + (phase_shift + uv.x) * 2.15;
    float wave_y = sin(angle) * amplitude + vertical_offset;
    float delta_y = wave_y - uv.y;
    float dist = abs(delta_y);

    if (invert_falloff) {
        if (delta_y > 0.0) dist *= 4.0;
    } else {
        if (delta_y < 0.0) dist *= 4.0;
    }

    float smooth_val = smoothstep(line_width * 1.5, 0.0, dist);
    float scale_val = pow(max(smooth_val, 0.0001), sharpness);
    float crest = smoothstep(line_width * 0.25, 0.0, dist);
    return min(base_color * scale_val + base_color * 0.25 * crest * scale_val, base_color * 1.25);
}

float4 main(PSIn input) : SV_Target {
    // Flip V so uv.y = 0 at bottom, 1 at top (matching GLSL gl_FragCoord)
    float2 uv = float2(input.uv.x, 1.0 - input.uv.y);

    // Subtle ultra-dark PSP midnight-cobalt radial glow fading to pure black at outer edges
    float2 centered = (uv - float2(0.5, 0.42)) * float2(1.0, 1.35);
    float bg_glow = exp(-dot(centered, centered) * 3.2);
    float3 col = float3(0.0, 0.015, 0.055) * bg_glow;

    // 7 layered asymmetric PSP XMB silk waves
    float3 c_cool = float3(0.18, 0.42, 0.72);
    float3 c_ice  = float3(0.28, 0.58, 0.88);
    float3 c_mist = float3(0.22, 0.36, 0.64);

    col += calc_sine(uv, 0.22, 0.20, 0.19, 0.00, 0.52, c_cool, 0.105, 14.0, false);
    col += calc_sine(uv, 0.38, 0.40, 0.15, 0.45, 0.50, c_ice,  0.100, 16.0, false);
    col += calc_sine(uv, 0.30, 0.60, 0.14, 1.10, 0.49, c_ice,  0.055, 22.0, false);

    col += calc_sine(uv, 0.14, 0.26, 0.08, 0.25, 0.34, c_mist, 0.105, 16.0, true);
    col += calc_sine(uv, 0.30, 0.36, 0.08, 0.85, 0.34, c_cool, 0.100, 16.0, true);
    col += calc_sine(uv, 0.46, 0.46, 0.07, 1.55, 0.33, c_ice,  0.055, 22.0, true);
    col += calc_sine(uv, 0.22, 0.58, 0.05, 2.20, 0.32, c_mist, 0.180, 14.0, true);

    float max_c = max(col.r, max(col.g, col.b));
    if (max_c > 0.002) {
        float n = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
        col += (n - 0.5) * (1.0 / 255.0);
    }
    return float4(saturate(col), 1.0);
}
"#;

pub struct PspWaveScene {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    cb: ID3D11Buffer,
    blend_state: ID3D11BlendState,
    rast_state: ID3D11RasterizerState,
}

impl PspWaveScene {
    pub fn new(device: &ID3D11Device) -> Result<Self, String> {
        let (_, vs, ps) = create_vs_ps(device, FULLSCREEN_VS_HLSL, PS_HLSL)?;
        let cb = create_buffer(
            device,
            size_of::<PspConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;
        let blend_state = create_blend_state(device, BlendMode::Opaque)?;
        let rast_state = create_rasterizer(device, false, false)?;
        Ok(Self {
            vs,
            ps,
            cb,
            blend_state,
            rast_state,
        })
    }
}

impl Scene for PspWaveScene {
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
        let time = if input.is_capture { 3.2 } else { input.elapsed };
        let consts = PspConsts {
            time,
            aspect: (input.width.max(1) as f32) / (input.height.max(1) as f32),
            _pad: [0.0; 2],
        };
        update_cbuffer(context, &self.cb, &consts);

        unsafe {
            let rtv_slice = [Some(rtv.clone())];
            context.OMSetRenderTargets(Some(&rtv_slice), None);
            set_viewport(context, input.width, input.height);
            context.OMSetBlendState(&self.blend_state, None, 0xFFFF_FFFF);
            context.RSSetState(&self.rast_state);
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vs, None);
            context.PSSetShader(&self.ps, None);
            let cbs = [Some(self.cb.clone())];
            context.PSSetConstantBuffers(0, Some(&cbs));
            context.Draw(3, 0);
        }
    }
}
