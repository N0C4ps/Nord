//! Structured shader reflection through DXC's native reflection API.

use crate::{ShaderBindingKind, ShaderResourceBinding};
use std::ffi::{CStr, OsStr, c_void};
use std::mem::transmute;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::Graphics::Direct3D::Dxc::{CLSID_DxcUtils, DXC_CP_ACP, DxcBuffer, IDxcUtils};
use windows::Win32::Graphics::Direct3D::{
    D3D_SIT_BYTEADDRESS, D3D_SIT_CBUFFER, D3D_SIT_RTACCELERATIONSTRUCTURE, D3D_SIT_SAMPLER,
    D3D_SIT_STRUCTURED, D3D_SIT_TBUFFER, D3D_SIT_TEXTURE, D3D_SIT_UAV_APPEND_STRUCTURED,
    D3D_SIT_UAV_CONSUME_STRUCTURED, D3D_SIT_UAV_FEEDBACKTEXTURE, D3D_SIT_UAV_RWBYTEADDRESS,
    D3D_SIT_UAV_RWSTRUCTURED, D3D_SIT_UAV_RWSTRUCTURED_WITH_COUNTER, D3D_SIT_UAV_RWTYPED,
};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_SHADER_DESC, D3D12_SHADER_INPUT_BIND_DESC, ID3D12ShaderReflection,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{GUID, Interface, PCSTR, PCWSTR};

type DxcCreateInstance = unsafe extern "system" fn(
    rclsid: *const GUID,
    riid: *const GUID,
    object: *mut *mut c_void,
) -> i32;

struct DxcLibrary(HMODULE);

impl Drop for DxcLibrary {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the reference returned by LoadLibraryW.
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}

/// Reflect the DXC `-Fre` artifact using the official DXC COM interfaces.
pub(crate) fn reflect_bindings(
    artifact: &[u8],
    compiler_executable: &OsStr,
) -> Result<Vec<ShaderResourceBinding>, String> {
    if artifact.is_empty() {
        return Err("DXC emitted an empty reflection artifact".into());
    }
    let library = load_dxc_library(compiler_executable)?;
    let proc_name = c"DxcCreateInstance";
    // SAFETY: the export is specified by DXC with the DxcCreateInstance ABI.
    let address = unsafe { GetProcAddress(library.0, PCSTR(proc_name.as_ptr().cast())) }
        .ok_or_else(|| "dxcompiler.dll does not export DxcCreateInstance".to_owned())?;
    // SAFETY: GetProcAddress returned the named DXC function, whose ABI is
    // represented by this exact function pointer signature.
    let create_instance: DxcCreateInstance = unsafe { transmute(address) };
    let mut utils_raw = std::ptr::null_mut();
    let utils_class = CLSID_DxcUtils;
    let utils_iid = IDxcUtils::IID;
    // SAFETY: the CLSID/IID are DXC's declared identifiers and the output
    // pointer is valid for receiving one COM interface reference.
    let status = unsafe {
        create_instance(&raw const utils_class, &raw const utils_iid, &raw mut utils_raw)
    };
    if status < 0 || utils_raw.is_null() {
        return Err(format!(
            "DxcCreateInstance(IDxcUtils) failed with HRESULT 0x{:08x}",
            status.cast_unsigned()
        ));
    }
    // SAFETY: successful DxcCreateInstance returns one owned IDxcUtils ref.
    let utils: IDxcUtils = unsafe { IDxcUtils::from_raw(utils_raw) };
    let buffer =
        DxcBuffer { Ptr: artifact.as_ptr().cast(), Size: artifact.len(), Encoding: DXC_CP_ACP.0 };
    let mut reflection_raw = std::ptr::null_mut();
    // SAFETY: artifact bytes remain live throughout the call and the requested
    // IID is ID3D12ShaderReflection's published interface identifier.
    unsafe {
        utils
            .CreateReflection(
                &raw const buffer,
                &ID3D12ShaderReflection::IID,
                &raw mut reflection_raw,
            )
            .map_err(|error| format!("DXC could not create shader reflection: {error}"))?;
    }
    if reflection_raw.is_null() {
        return Err("DXC returned a null shader reflection interface".into());
    }
    // SAFETY: CreateReflection succeeded and returned the requested interface.
    let reflection: ID3D12ShaderReflection =
        unsafe { ID3D12ShaderReflection::from_raw(reflection_raw) };
    let mut shader_desc = D3D12_SHADER_DESC::default();
    // SAFETY: reflection is a valid DXC-owned interface and shader_desc is writable.
    unsafe { reflection.GetDesc(&raw mut shader_desc) }
        .map_err(|error| format!("could not read shader reflection header: {error}"))?;
    let mut bindings = Vec::with_capacity(shader_desc.BoundResources as usize);
    for index in 0..shader_desc.BoundResources {
        let mut reflected = D3D12_SHADER_INPUT_BIND_DESC::default();
        // SAFETY: index is bounded by BoundResources returned by this reflection object.
        unsafe { reflection.GetResourceBindingDesc(index, &raw mut reflected) }
            .map_err(|error| format!("could not read shader binding {index}: {error}"))?;
        let kind = match reflected.Type {
            D3D_SIT_CBUFFER => ShaderBindingKind::ConstantBuffer,
            D3D_SIT_TBUFFER
            | D3D_SIT_TEXTURE
            | D3D_SIT_STRUCTURED
            | D3D_SIT_BYTEADDRESS
            | D3D_SIT_RTACCELERATIONSTRUCTURE => ShaderBindingKind::SampledResource,
            D3D_SIT_UAV_RWTYPED
            | D3D_SIT_UAV_RWSTRUCTURED
            | D3D_SIT_UAV_RWBYTEADDRESS
            | D3D_SIT_UAV_APPEND_STRUCTURED
            | D3D_SIT_UAV_CONSUME_STRUCTURED
            | D3D_SIT_UAV_RWSTRUCTURED_WITH_COUNTER
            | D3D_SIT_UAV_FEEDBACKTEXTURE => ShaderBindingKind::StorageResource,
            D3D_SIT_SAMPLER => ShaderBindingKind::Sampler,
            _ => {
                return Err(format!(
                    "shader binding {index} has unsupported DXC resource type {:?}",
                    reflected.Type
                ));
            }
        };
        if reflected.BindCount == 0 || reflected.BindCount == u32::MAX {
            return Err(format!(
                "shader binding {index} has an unbounded or invalid descriptor count"
            ));
        }
        if reflected.BindPoint.checked_add(reflected.BindCount).is_none() {
            return Err(format!("shader binding {index} register range overflows"));
        }
        if reflected.Name.0.is_null() {
            return Err(format!("shader binding {index} has no resource name"));
        }
        // SAFETY: DXC owns a NUL-terminated resource name for the lifetime of
        // the reflection interface, which remains alive until this function ends.
        let name =
            unsafe { CStr::from_ptr(reflected.Name.0.cast()) }.to_string_lossy().into_owned();
        bindings.push(ShaderResourceBinding {
            name,
            kind,
            register: reflected.BindPoint,
            space: reflected.Space,
            count: reflected.BindCount,
        });
    }
    Ok(bindings)
}

fn load_dxc_library(compiler_executable: &OsStr) -> Result<DxcLibrary, String> {
    let compiler_path = PathBuf::from(compiler_executable);
    let sibling_library = compiler_path.with_file_name("dxcompiler.dll");
    let candidates = [
        std::env::var_os("NORD_DXCOMPILER_PATH").map(PathBuf::from),
        sibling_library.is_file().then_some(sibling_library),
        Some(PathBuf::from("dxcompiler.dll")),
    ];
    let mut last_error = None;
    for candidate in candidates.into_iter().flatten() {
        let wide = candidate.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        // SAFETY: wide is NUL-terminated and remains live through LoadLibraryW.
        match unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) } {
            Ok(module) => return Ok(DxcLibrary(module)),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(format!(
        "could not load dxcompiler.dll for structured reflection: {}",
        last_error.unwrap_or_else(|| "no library path was available".into())
    ))
}
