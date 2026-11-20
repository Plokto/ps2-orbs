use crate::config::SceneId;
use crate::scenes::{create_scene, FrameInput, Scene};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use windows::core::{HRESULT, Interface};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
    ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice1, IDXGIFactory2, IDXGISwapChain1, DXGI_ERROR_DEVICE_REMOVED,
    DXGI_ERROR_DEVICE_RESET, DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, DXGI_USAGE_SHADER_INPUT,
};

pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    rtv: Option<ID3D11RenderTargetView>,
    backbuf_srv: Option<ID3D11ShaderResourceView>,
    buffer_count: u32,
    active_scene_id: SceneId,
    active_scene: Option<Box<dyn Scene>>,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new(
        hwnd: HWND,
        width: u32,
        height: u32,
        initial_scene: SceneId,
    ) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);

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
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT | DXGI_USAGE_SHADER_INPUT,
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
                    sc_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
                    match factory.CreateSwapChainForHwnd(&device, hwnd, &sc_desc, None, None) {
                        Ok(sc) => sc,
                        Err(_) => {
                            buffer_count = 1;
                            sc_desc.BufferCount = 1;
                            sc_desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
                            sc_desc.BufferUsage =
                                DXGI_USAGE_RENDER_TARGET_OUTPUT | DXGI_USAGE_SHADER_INPUT;
                            factory
                                .CreateSwapChainForHwnd(&device, hwnd, &sc_desc, None, None)
                                .or_else(|_| {
                                    sc_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
                                    factory.CreateSwapChainForHwnd(&device, hwnd, &sc_desc, None, None)
                                })
                                .map_err(|e| format!("CreateSwapChainForHwnd failed: {e}"))?
                        }
                    }
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
            let mut backbuf_srv: Option<ID3D11ShaderResourceView> = None;
            let _ = device.CreateShaderResourceView(&backbuf, None, Some(&mut backbuf_srv));

            let active_scene = Some(create_scene(initial_scene, &device, width, height)?);

            Ok(Self {
                device,
                context,
                swap_chain,
                rtv,
                backbuf_srv,
                buffer_count,
                active_scene_id: initial_scene,
                active_scene,
                width,
                height,
            })
        }
    }

    pub fn set_scene(&mut self, id: SceneId) -> Result<(), String> {
        if self.active_scene_id == id && self.active_scene.is_some() {
            return Ok(());
        }
        // Drop old scene first so its GPU resources are immediately released before allocating new ones.
        self.active_scene = None;
        unsafe {
            self.context.ClearState();
            self.context.Flush();
        }
        let scene = create_scene(id, &self.device, self.width, self.height)?;
        self.active_scene = Some(scene);
        self.active_scene_id = id;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return Ok(());
        }
        unsafe {
            self.rtv = None;
            self.backbuf_srv = None;
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
            let mut backbuf_srv: Option<ID3D11ShaderResourceView> = None;
            let _ = self
                .device
                .CreateShaderResourceView(&backbuf, None, Some(&mut backbuf_srv));
            self.backbuf_srv = backbuf_srv;
            self.width = width;
            self.height = height;
            if let Some(ref mut scene) = self.active_scene {
                scene.resize(&self.device, width, height)?;
            }
            Ok(())
        }
    }

    pub fn draw_and_present(&mut self, input: &FrameInput) -> Result<(), HRESULT> {
        self.draw_only(input);
        let hr = unsafe { self.swap_chain.Present(0, DXGI_PRESENT(0)) };
        if hr.is_err() {
            return Err(hr);
        }
        Ok(())
    }

    pub fn draw_only(&mut self, input: &FrameInput) {
        let (Some(ref rtv), Some(ref mut scene)) = (&self.rtv, &mut self.active_scene) else {
            return;
        };
        scene.render(&self.context, rtv, self.backbuf_srv.as_ref(), input);
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
