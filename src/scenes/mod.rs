pub mod ps2_orbs;
pub mod ps2_towers;
pub mod ps3_xmb;
pub mod ps4_flow;
pub mod psp_wave;

use crate::config::SceneId;
use std::ffi::c_void;
use std::mem::{size_of, transmute};
use windows::core::{s, w, HRESULT, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE, TRUE};
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11ShaderResourceView, ID3D11VertexShader,
    D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
    D3D11_BLEND_SRC_ALPHA, D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_CPU_ACCESS_WRITE,
    D3D11_CULL_BACK, D3D11_CULL_NONE, D3D11_FILL_SOLID, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_WRITE_DISCARD, D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC,
    D3D11_SUBRESOURCE_DATA, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_VIEWPORT,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[derive(Clone, Copy, Debug)]
pub struct FrameInput {
    pub clock_sec: f64,
    pub month: u32,
    pub day: u32,
    pub dt: f32,
    pub elapsed: f32,
    pub width: u32,
    pub height: u32,
    pub is_capture: bool,
}

pub trait Scene {
    fn resize(&mut self, device: &ID3D11Device, width: u32, height: u32) -> Result<(), String>;
    fn render(
        &mut self,
        context: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        backbuf_srv: Option<&ID3D11ShaderResourceView>,
        input: &FrameInput,
    );
}

pub fn create_scene(
    id: SceneId,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<Box<dyn Scene>, String> {
    match id {
        SceneId::Ps2Original => Ok(Box::new(ps2_orbs::Ps2OrbsScene::new(device, false)?)),
        SceneId::Ps2Vivid => Ok(Box::new(ps2_orbs::Ps2OrbsScene::new(device, true)?)),
        SceneId::Ps2Towers => Ok(Box::new(ps2_towers::Ps2TowersScene::new(
            device, width, height,
        )?)),
        SceneId::Ps3Oled => Ok(Box::new(ps3_xmb::Ps3XmbScene::new(device, true)?)),
        SceneId::Ps3Classic => Ok(Box::new(ps3_xmb::Ps3XmbScene::new(device, false)?)),
        SceneId::PspWave => Ok(Box::new(psp_wave::PspWaveScene::new(device)?)),
        SceneId::Ps4Flow => Ok(Box::new(ps4_flow::Ps4FlowScene::new(device)?)),
    }
}

pub const FULLSCREEN_VS_HLSL: &str = r#"
struct VSOut {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};
VSOut main(uint vid : SV_VertexID) {
    VSOut o;
    float2 uv = float2((vid << 1) & 2, vid & 2);
    o.uv = uv;
    o.pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    return o;
}
"#;

type PfnD3DCompile = unsafe extern "system" fn(
    p_src_data: *const c_void,
    src_data_size: usize,
    p_source_name: PCSTR,
    p_defines: *const c_void,
    p_include: *const c_void,
    p_entrypoint: PCSTR,
    p_target: PCSTR,
    flags1: u32,
    flags2: u32,
    pp_code: *mut Option<ID3DBlob>,
    pp_error_msgs: *mut Option<ID3DBlob>,
) -> HRESULT;

pub struct ShaderCompiler {
    lib: HMODULE,
    d3d_compile: PfnD3DCompile,
}

impl ShaderCompiler {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let lib = LoadLibraryW(w!("d3dcompiler_47.dll"))
                .map_err(|e| format!("LoadLibraryW(d3dcompiler_47.dll) failed: {e}"))?;
            let Some(proc_addr) = GetProcAddress(lib, s!("D3DCompile")) else {
                let _ = FreeLibrary(lib);
                return Err("D3DCompile not found in d3dcompiler_47.dll".to_string());
            };
            let d3d_compile: PfnD3DCompile = transmute(proc_addr);
            Ok(Self {
                lib,
                d3d_compile,
            })
        }
    }

    pub fn compile(&self, src: &str, target: PCSTR) -> Result<Vec<u8>, String> {
        unsafe {
            let mut code_blob: Option<ID3DBlob> = None;
            let mut err_blob: Option<ID3DBlob> = None;
            let hr = (self.d3d_compile)(
                src.as_ptr() as *const c_void,
                src.len(),
                PCSTR::null(),
                std::ptr::null(),
                std::ptr::null(),
                s!("main"),
                target,
                1 << 15, // D3DCOMPILE_OPTIMIZATION_LEVEL3
                0,
                &mut code_blob,
                &mut err_blob,
            );
            if hr.is_err() {
                let msg = if let Some(eb) = err_blob.take() {
                    let ptr = eb.GetBufferPointer() as *const u8;
                    let len = eb.GetBufferSize();
                    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned();
                    drop(eb);
                    s
                } else {
                    format!("HRESULT 0x{:08x}", hr.0 as u32)
                };
                drop(code_blob);
                return Err(msg);
            }
            drop(err_blob);
            let blob = code_blob.ok_or_else(|| "Empty shader blob".to_string())?;
            let ptr = blob.GetBufferPointer() as *const u8;
            let len = blob.GetBufferSize();
            let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
            drop(blob);
            Ok(bytes)
        }
    }

    pub fn create_vs(
        &self,
        device: &ID3D11Device,
        src: &str,
    ) -> Result<(Vec<u8>, ID3D11VertexShader), String> {
        let bytes = self.compile(src, s!("vs_4_0"))?;
        unsafe {
            let mut vs: Option<ID3D11VertexShader> = None;
            device
                .CreateVertexShader(&bytes, None, Some(&mut vs))
                .map_err(|e| format!("CreateVertexShader failed: {e}"))?;
            Ok((bytes, vs.ok_or("No VS")?))
        }
    }

    pub fn create_ps(&self, device: &ID3D11Device, src: &str) -> Result<ID3D11PixelShader, String> {
        let bytes = self.compile(src, s!("ps_4_0"))?;
        unsafe {
            let mut ps: Option<ID3D11PixelShader> = None;
            device
                .CreatePixelShader(&bytes, None, Some(&mut ps))
                .map_err(|e| format!("CreatePixelShader failed: {e}"))?;
            ps.ok_or_else(|| "No PS".to_string())
        }
    }
}

impl Drop for ShaderCompiler {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.lib);
        }
    }
}

pub fn compile_hlsl(vs_src: &str, ps_src: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let compiler = ShaderCompiler::new()?;
    let vs_bytes = compiler.compile(vs_src, s!("vs_4_0"))?;
    let ps_bytes = compiler.compile(ps_src, s!("ps_4_0"))?;
    Ok((vs_bytes, ps_bytes))
}

pub fn create_vs_ps(
    device: &ID3D11Device,
    vs_src: &str,
    ps_src: &str,
) -> Result<(Vec<u8>, ID3D11VertexShader, ID3D11PixelShader), String> {
    let (vs_bytes, ps_bytes) = compile_hlsl(vs_src, ps_src)?;
    unsafe {
        let mut vs: Option<ID3D11VertexShader> = None;
        device
            .CreateVertexShader(&vs_bytes, None, Some(&mut vs))
            .map_err(|e| format!("CreateVertexShader failed: {e}"))?;
        let mut ps: Option<ID3D11PixelShader> = None;
        device
            .CreatePixelShader(&ps_bytes, None, Some(&mut ps))
            .map_err(|e| format!("CreatePixelShader failed: {e}"))?;
        Ok((
            vs_bytes,
            vs.ok_or("No VS")?,
            ps.ok_or("No PS")?,
        ))
    }
}

#[derive(Clone, Copy)]
pub enum BlendMode {
    Opaque,
    Alpha,
    Additive,
}

pub fn create_blend_state(device: &ID3D11Device, mode: BlendMode) -> Result<ID3D11BlendState, String> {
    let mut desc = D3D11_BLEND_DESC::default();
    desc.RenderTarget[0] = match mode {
        BlendMode::Opaque => D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: false.into(),
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        },
        BlendMode::Alpha => D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: TRUE,
            SrcBlend: D3D11_BLEND_SRC_ALPHA,
            DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D11_BLEND_OP_ADD,
            SrcBlendAlpha: D3D11_BLEND_ONE,
            DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
        },
        BlendMode::Additive => D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: TRUE,
            SrcBlend: D3D11_BLEND_ONE,
            DestBlend: D3D11_BLEND_ONE,
            BlendOp: D3D11_BLEND_OP_ADD,
            SrcBlendAlpha: D3D11_BLEND_ONE,
            DestBlendAlpha: D3D11_BLEND_ONE,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
        },
    };
    unsafe {
        let mut bs: Option<ID3D11BlendState> = None;
        device
            .CreateBlendState(&desc, Some(&mut bs))
            .map_err(|e| format!("CreateBlendState failed: {e}"))?;
        bs.ok_or_else(|| "No blend state".to_string())
    }
}

pub fn create_rasterizer(
    device: &ID3D11Device,
    cull_back: bool,
    depth_clip: bool,
) -> Result<ID3D11RasterizerState, String> {
    let desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: if cull_back { D3D11_CULL_BACK } else { D3D11_CULL_NONE },
        FrontCounterClockwise: false.into(),
        DepthBias: 0,
        DepthBiasClamp: 0.0,
        SlopeScaledDepthBias: 0.0,
        DepthClipEnable: depth_clip.into(),
        ScissorEnable: false.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
    };
    unsafe {
        let mut rs: Option<ID3D11RasterizerState> = None;
        device
            .CreateRasterizerState(&desc, Some(&mut rs))
            .map_err(|e| format!("CreateRasterizerState failed: {e}"))?;
        rs.ok_or_else(|| "No rasterizer state".to_string())
    }
}

pub fn create_buffer(
    device: &ID3D11Device,
    byte_width: u32,
    bind_flags: u32,
    dynamic: bool,
    init_data: Option<*const c_void>,
) -> Result<ID3D11Buffer, String> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: byte_width,
        Usage: if dynamic { D3D11_USAGE_DYNAMIC } else { D3D11_USAGE_DEFAULT },
        BindFlags: bind_flags,
        CPUAccessFlags: if dynamic { D3D11_CPU_ACCESS_WRITE.0 as u32 } else { 0 },
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let sub = init_data.map(|ptr| D3D11_SUBRESOURCE_DATA {
        pSysMem: ptr,
        SysMemPitch: 0,
        SysMemSlicePitch: 0,
    });
    unsafe {
        let mut buf: Option<ID3D11Buffer> = None;
        device
            .CreateBuffer(&desc, sub.as_ref().map(|s| s as *const _), Some(&mut buf))
            .map_err(|e| format!("CreateBuffer failed: {e}"))?;
        buf.ok_or_else(|| "No buffer".to_string())
    }
}

pub fn update_dynamic_slice<T: Copy>(
    context: &ID3D11DeviceContext,
    buf: &ID3D11Buffer,
    data: &[T],
) {
    if data.is_empty() {
        return;
    }
    unsafe {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if context
            .Map(buf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
            .is_ok()
        {
            std::ptr::copy_nonoverlapping(data.as_ptr(), mapped.pData as *mut T, data.len());
            context.Unmap(buf, 0);
        }
    }
}

pub fn update_cbuffer<T: Copy>(context: &ID3D11DeviceContext, buf: &ID3D11Buffer, data: &T) {
    unsafe {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if context
            .Map(buf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
            .is_ok()
        {
            std::ptr::copy_nonoverlapping(data as *const T as *const u8, mapped.pData as *mut u8, size_of::<T>());
            context.Unmap(buf, 0);
        }
    }
}

pub fn set_viewport(context: &ID3D11DeviceContext, width: u32, height: u32) {
    let vp = D3D11_VIEWPORT {
        TopLeftX: 0.0,
        TopLeftY: 0.0,
        Width: width.max(1) as f32,
        Height: height.max(1) as f32,
        MinDepth: 0.0,
        MaxDepth: 1.0,
    };
    unsafe {
        context.RSSetViewports(Some(&[vp]));
    }
}
