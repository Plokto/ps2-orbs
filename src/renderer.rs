use crate::orbs::{Vertex, MAX_VERTICES};
use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::mem::{size_of, transmute};
use std::path::Path;
use windows::core::{s, w, HRESULT, Interface, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HWND, TRUE};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0,
    D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext,
    ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView,
    ID3D11Texture2D, ID3D11VertexShader, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_BIND_VERTEX_BUFFER,
    D3D11_BLEND_DESC, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BUFFER_DESC,
    D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_CPU_ACCESS_READ, D3D11_CPU_ACCESS_WRITE,
    D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_CULL_NONE, D3D11_FILL_SOLID,
    D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_MAP_WRITE_DISCARD, D3D11_RASTERIZER_DESC,
    D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DYNAMIC,
    D3D11_USAGE_STAGING, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32G32B32A32_FLOAT,
    DXGI_FORMAT_R32G32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice1, IDXGIFactory2, IDXGISwapChain1, DXGI_ERROR_DEVICE_REMOVED,
    DXGI_ERROR_DEVICE_RESET, DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const VS_HLSL_CLEAN: &str = r#"
struct VSIn {
    float2 pos    : POSITION;
    float2 uv     : TEXCOORD0;
    float4 color  : COLOR0;
    float2 params : TEXCOORD1;
};
struct VSOut {
    float4 pos    : SV_Position;
    float2 uv     : TEXCOORD0;
    float4 color  : COLOR0;
    float2 params : TEXCOORD1;
};
VSOut main(VSIn input) {
    VSOut output;
    output.pos    = float4(input.pos, 0.0, 1.0);
    output.uv     = input.uv;
    output.color  = input.color;
    output.params = input.params;
    return output;
}
"#;

const PS_HLSL: &str = r#"
struct PSIn {
    float4 pos    : SV_Position;
    float2 uv     : TEXCOORD0;
    float4 color  : COLOR0;
    float2 params : TEXCOORD1;
};
float4 main(PSIn input) : SV_Target {
    float r2 = dot(input.uv, input.uv);
    float d = saturate(1.0 - r2);
    float glow = (d > 0.0) ? pow(d, input.params.x) : 0.0;
    float core_mask = 1.0 - smoothstep(0.65, 1.0, r2);
    float core = input.params.y * exp(-r2 * 14.0) * core_mask;
    float3 col = input.color.rgb * glow + float3(0.92, 0.98, 1.0) * core;

    float max_c = max(col.r, max(col.g, col.b));
    if (max_c > 0.002) {
        float2 p = input.pos.xy;
        float n = frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453);
        col += (n - 0.5) * (1.0 / 255.0);
    }
    return float4(max(col, 0.0), 1.0);
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

fn compile_shaders_and_unload() -> Result<(Vec<u8>, Vec<u8>), String> {
    unsafe {
        let lib = LoadLibraryW(w!("d3dcompiler_47.dll"))
            .map_err(|e| format!("LoadLibraryW(d3dcompiler_47.dll) failed: {e}"))?;
        let proc = GetProcAddress(lib, s!("D3DCompile"));
        let Some(proc_addr) = proc else {
            let _ = FreeLibrary(lib);
            return Err("D3DCompile not found in d3dcompiler_47.dll".to_string());
        };
        let d3d_compile: PfnD3DCompile = transmute(proc_addr);

        let compile_one = |src: &str, target: PCSTR| -> Result<Vec<u8>, String> {
            let mut code_blob: Option<ID3DBlob> = None;
            let mut err_blob: Option<ID3DBlob> = None;
            let hr = d3d_compile(
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
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let s = String::from_utf8_lossy(slice).into_owned();
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
        };

        let vs_res = compile_one(VS_HLSL_CLEAN, s!("vs_4_0"));
        let ps_res = compile_one(PS_HLSL, s!("ps_4_0"));
        let _ = FreeLibrary(lib);

        Ok((vs_res?, ps_res?))
    }
}

pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    rtv: Option<ID3D11RenderTargetView>,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    vb: ID3D11Buffer,
    blend_state: ID3D11BlendState,
    rast_state: ID3D11RasterizerState,
    buffer_count: u32,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);

        let (vs_bytes, ps_bytes) = compile_shaders_and_unload()?;

        unsafe {
            let feature_levels = [D3D_FEATURE_LEVEL_11_0];
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_SINGLETHREADED,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| format!("D3D11CreateDevice failed: {e}"))?;

            let device = device.ok_or("No D3D11 device")?;
            let context = context.ok_or("No D3D11 context")?;

            let dxgi_device: IDXGIDevice1 = device
                .cast()
                .map_err(|e| format!("Cast IDXGIDevice1 failed: {e}"))?;
            let _ = dxgi_device.SetMaximumFrameLatency(1);

            let adapter: IDXGIAdapter = dxgi_device
                .GetAdapter()
                .map_err(|e| format!("GetAdapter failed: {e}"))?;
            let factory: IDXGIFactory2 = adapter
                .GetParent()
                .map_err(|e| format!("GetParent(IDXGIFactory2) failed: {e}"))?;

            let mut sc_desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };

            let mut buffer_count = 2u32;
            let swap_chain = match factory.CreateSwapChainForHwnd(&device, hwnd, &sc_desc, None, None) {
                Ok(sc) => sc,
                Err(_) => {
                    buffer_count = 1;
                    sc_desc.BufferCount = 1;
                    sc_desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
                    factory
                        .CreateSwapChainForHwnd(&device, hwnd, &sc_desc, None, None)
                        .map_err(|e| format!("CreateSwapChainForHwnd failed: {e}"))?
                }
            };

            let _ = factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER);

            let backbuf: ID3D11Texture2D = swap_chain
                .GetBuffer(0)
                .map_err(|e| format!("GetBuffer(0) failed: {e}"))?;
            let mut rtv: Option<ID3D11RenderTargetView> = None;
            device
                .CreateRenderTargetView(&backbuf, None, Some(&mut rtv))
                .map_err(|e| format!("CreateRenderTargetView failed: {e}"))?;

            let mut vs: Option<ID3D11VertexShader> = None;
            device
                .CreateVertexShader(&vs_bytes, None, Some(&mut vs))
                .map_err(|e| format!("CreateVertexShader failed: {e}"))?;
            let vs = vs.ok_or("No VS")?;

            let mut ps: Option<ID3D11PixelShader> = None;
            device
                .CreatePixelShader(&ps_bytes, None, Some(&mut ps))
                .map_err(|e| format!("CreatePixelShader failed: {e}"))?;
            let ps = ps.ok_or("No PS")?;

            let layout_desc = [
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
                    AlignedByteOffset: D3D11_APPEND_ALIGNED_ELEMENT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("COLOR"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: D3D11_APPEND_ALIGNED_ELEMENT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("TEXCOORD"),
                    SemanticIndex: 1,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: D3D11_APPEND_ALIGNED_ELEMENT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];

            let mut input_layout: Option<ID3D11InputLayout> = None;
            device
                .CreateInputLayout(&layout_desc, &vs_bytes, Some(&mut input_layout))
                .map_err(|e| format!("CreateInputLayout failed: {e}"))?;
            let input_layout = input_layout.ok_or("No input layout")?;

            let vb_desc = D3D11_BUFFER_DESC {
                ByteWidth: (MAX_VERTICES * size_of::<Vertex>()) as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let mut vb: Option<ID3D11Buffer> = None;
            device
                .CreateBuffer(&vb_desc, None, Some(&mut vb))
                .map_err(|e| format!("CreateBuffer failed: {e}"))?;
            let vb = vb.ok_or("No VB")?;

            let mut blend_desc = D3D11_BLEND_DESC::default();
            blend_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: TRUE,
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_ONE,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_ONE,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut blend_state: Option<ID3D11BlendState> = None;
            device
                .CreateBlendState(&blend_desc, Some(&mut blend_state))
                .map_err(|e| format!("CreateBlendState failed: {e}"))?;
            let blend_state = blend_state.ok_or("No blend state")?;

            let rast_desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                FrontCounterClockwise: false.into(),
                DepthBias: 0,
                DepthBiasClamp: 0.0,
                SlopeScaledDepthBias: 0.0,
                DepthClipEnable: false.into(),
                ScissorEnable: false.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
            };
            let mut rast_state: Option<ID3D11RasterizerState> = None;
            device
                .CreateRasterizerState(&rast_desc, Some(&mut rast_state))
                .map_err(|e| format!("CreateRasterizerState failed: {e}"))?;
            let rast_state = rast_state.ok_or("No rasterizer state")?;

            Ok(Self {
                device,
                context,
                swap_chain,
                rtv,
                vs,
                ps,
                input_layout,
                vb,
                blend_state,
                rast_state,
                buffer_count,
                width,
                height,
            })
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return Ok(());
        }
        unsafe {
            self.rtv = None;
            self.context.ClearState();
            self.context.Flush();
            self.swap_chain
                .ResizeBuffers(
                    self.buffer_count,
                    width,
                    height,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )
                .map_err(|e| format!("ResizeBuffers failed: {e}"))?;

            let backbuf: ID3D11Texture2D = self
                .swap_chain
                .GetBuffer(0)
                .map_err(|e| format!("GetBuffer(0) after resize failed: {e}"))?;
            let mut rtv: Option<ID3D11RenderTargetView> = None;
            self.device
                .CreateRenderTargetView(&backbuf, None, Some(&mut rtv))
                .map_err(|e| format!("CreateRenderTargetView after resize failed: {e}"))?;
            self.rtv = rtv;
            self.width = width;
            self.height = height;
            Ok(())
        }
    }

    pub fn draw_and_present(&self, verts: &[Vertex]) -> Result<(), HRESULT> {
        self.draw_only(verts);
        let hr = unsafe { self.swap_chain.Present(0, DXGI_PRESENT(0)) };
        if hr.is_err() {
            return Err(hr);
        }
        Ok(())
    }

    pub fn draw_only(&self, verts: &[Vertex]) {
        let Some(ref rtv) = self.rtv else {
            return;
        };
        unsafe {
            let rtv_slice = [Some(rtv.clone())];
            self.context.OMSetRenderTargets(Some(&rtv_slice), None);
            self.context
                .ClearRenderTargetView(rtv, &[0.0, 0.0, 0.0, 1.0]);

            let vp = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context.RSSetViewports(Some(&[vp]));
            self.context
                .OMSetBlendState(&self.blend_state, None, 0xFFFF_FFFF);
            self.context.RSSetState(&self.rast_state);
            self.context.IASetInputLayout(&self.input_layout);
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vs, None);
            self.context.PSSetShader(&self.ps, None);

            let count = verts.len().min(MAX_VERTICES);
            if count > 0 {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                if self
                    .context
                    .Map(&self.vb, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                    .is_ok()
                {
                    std::ptr::copy_nonoverlapping(
                        verts.as_ptr(),
                        mapped.pData as *mut Vertex,
                        count,
                    );
                    self.context.Unmap(&self.vb, 0);

                    let stride = size_of::<Vertex>() as u32;
                    let offset = 0u32;
                    let vbs = [Some(self.vb.clone())];
                    self.context
                        .IASetVertexBuffers(0, 1, Some(vbs.as_ptr()), Some(&stride), Some(&offset));
                    self.context.Draw(count as u32, 0);
                }
            }
        }
    }

    pub fn capture_bmp(&self, path: &Path) -> Result<(), String> {
        unsafe {
            let backbuf: ID3D11Texture2D = self
                .swap_chain
                .GetBuffer(0)
                .map_err(|e| format!("GetBuffer(0) for capture failed: {e}"))?;

            let desc = D3D11_TEXTURE2D_DESC {
                Width: self.width,
                Height: self.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };

            let mut staging: Option<ID3D11Texture2D> = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .map_err(|e| format!("CreateTexture2D staging failed: {e}"))?;
            let staging = staging.ok_or("No staging texture")?;

            self.context.CopyResource(&staging, &backbuf);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| format!("Map staging failed: {e}"))?;

            let w = self.width as usize;
            let h = self.height as usize;
            let row_bytes = (w * 3 + 3) & !3;
            let img_size = row_bytes * h;
            let file_size = 54 + img_size;

            let mut file = File::create(path).map_err(|e| format!("Create BMP failed: {e}"))?;
            let mut header = [0u8; 54];
            header[0] = b'B';
            header[1] = b'M';
            header[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
            header[10..14].copy_from_slice(&54u32.to_le_bytes());
            header[14..18].copy_from_slice(&40u32.to_le_bytes());
            header[18..22].copy_from_slice(&(self.width as i32).to_le_bytes());
            header[22..26].copy_from_slice(&(self.height as i32).to_le_bytes());
            header[26..28].copy_from_slice(&1u16.to_le_bytes());
            header[28..30].copy_from_slice(&24u16.to_le_bytes());
            header[34..38].copy_from_slice(&(img_size as u32).to_le_bytes());
            file.write_all(&header)
                .map_err(|e| format!("Write BMP header failed: {e}"))?;

            let src_base = mapped.pData as *const u8;
            let pitch = mapped.RowPitch as usize;
            let mut row_buf = vec![0u8; row_bytes];
            for y_bmp in 0..h {
                let src_y = h - 1 - y_bmp;
                let src_row = std::slice::from_raw_parts(src_base.add(src_y * pitch), w * 4);
                for x in 0..w {
                    row_buf[x * 3] = src_row[x * 4];
                    row_buf[x * 3 + 1] = src_row[x * 4 + 1];
                    row_buf[x * 3 + 2] = src_row[x * 4 + 2];
                }
                file.write_all(&row_buf)
                    .map_err(|e| format!("Write BMP row failed: {e}"))?;
            }

            self.context.Unmap(&staging, 0);
            Ok(())
        }
    }
}

pub fn is_device_lost(hr: HRESULT) -> bool {
    hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET
}
