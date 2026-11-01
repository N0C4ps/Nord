//! DXC shader compilation with explicit targets, defines, diagnostics and
//! compiler-produced reflection/disassembly artifacts.

use crate::ShaderStage;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// Source shader compilation request.
#[derive(Debug, Clone)]
pub struct ShaderCompileOptions {
    pub source: PathBuf,
    pub stage: ShaderStage,
    pub entry_point: String,
    pub target_profile: String,
    pub include_directories: Vec<PathBuf>,
    pub defines: Vec<(String, String)>,
    pub debug_information: bool,
}

impl ShaderCompileOptions {
    /// Construct a shader-model-6 compilation request for the selected stage.
    #[must_use]
    pub fn new(
        source: impl Into<PathBuf>,
        stage: ShaderStage,
        entry_point: impl Into<String>,
    ) -> Self {
        let target_profile = match stage {
            ShaderStage::Vertex => "vs_6_0",
            ShaderStage::Pixel => "ps_6_0",
            ShaderStage::Compute => "cs_6_0",
        };
        Self {
            source: source.into(),
            stage,
            entry_point: entry_point.into(),
            target_profile: target_profile.into(),
            include_directories: Vec::new(),
            defines: Vec::new(),
            debug_information: cfg!(debug_assertions),
        }
    }
}

/// Human-readable compiler warning or error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderDiagnostic {
    pub message: String,
    pub is_error: bool,
}

/// DXC reflection output and textual dump for tooling/diagnostic inspection.
/// The binary reflection chunk is preserved exactly for backend-native
/// reflection APIs; the dump is compiler-version-specific diagnostic text.
#[derive(Debug, Clone)]
pub struct ShaderReflection {
    binary: Vec<u8>,
    dump: String,
    bindings: Vec<ShaderResourceBinding>,
}

/// Resource namespace reflected from the compiled DXIL shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderBindingKind {
    ConstantBuffer,
    SampledResource,
    StorageResource,
    Sampler,
}

/// One optimized, active HLSL binding from the DXC reflection table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShaderResourceBinding {
    pub name: String,
    pub kind: ShaderBindingKind,
    pub register: u32,
    pub space: u32,
    pub count: u32,
}

impl ShaderReflection {
    #[must_use]
    pub fn binary(&self) -> &[u8] {
        &self.binary
    }
    #[must_use]
    pub fn dump(&self) -> &str {
        &self.dump
    }

    /// Compiled resource bindings with register namespace, space and count.
    #[must_use]
    pub fn bindings(&self) -> &[ShaderResourceBinding] {
        &self.bindings
    }
}

/// Compiled shader bytecode and its content identity.
#[derive(Debug, Clone)]
pub struct CompiledShader {
    pub stage: ShaderStage,
    pub shader_id: crate::ShaderId,
    pub bytecode: Vec<u8>,
    pub reflection: ShaderReflection,
    pub diagnostics: Vec<ShaderDiagnostic>,
}

/// DXC command-line compiler. `NORD_DXC_PATH` can point to a pinned compiler;
/// otherwise `dxc` is resolved from PATH. Pinning the compiler in a toolchain
/// manifest is recommended for reproducible production builds.
#[derive(Debug, Clone)]
pub struct ShaderCompiler {
    executable: OsString,
}
impl Default for ShaderCompiler {
    fn default() -> Self {
        let executable = std::env::var_os("NORD_DXC_PATH")
            .or_else(|| std::env::var_os("DXC_PATH"))
            .or_else(find_windows_sdk_dxc)
            .unwrap_or_else(|| OsString::from("dxc"));
        Self { executable }
    }
}

impl ShaderCompiler {
    /// Select an explicit DXC executable path.
    #[must_use]
    pub fn with_executable(path: impl Into<OsString>) -> Self {
        Self { executable: path.into() }
    }
    /// Compile source and emit DXIL plus a reflection container and disassembly.
    ///
    /// # Errors
    /// Returns source validation, compiler launch/compilation, or artifact I/O failures.
    pub fn compile(
        &self,
        options: &ShaderCompileOptions,
    ) -> Result<CompiledShader, ShaderCompileError> {
        validate_options(options)?;
        if !options.source.is_file() {
            return Err(ShaderCompileError::SourceNotFound(options.source.clone()));
        }
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let temp_base =
            std::env::temp_dir().join(format!("nord-shader-{}-{id}", std::process::id()));
        let object_path = temp_base.with_extension("dxil");
        let reflection_path = temp_base.with_extension("reflection");
        let dump_path = temp_base.with_extension("asm");
        let mut command = Command::new(&self.executable);
        command
            .arg(options.source.as_os_str())
            .arg("-E")
            .arg(&options.entry_point)
            .arg("-T")
            .arg(&options.target_profile)
            .arg("-Fo")
            .arg(&object_path)
            .arg("-Fre")
            .arg(&reflection_path)
            .arg("-Fc")
            .arg(&dump_path);
        if options.debug_information {
            command.args(["-Zi", "-Qembed_debug"]);
        } else {
            command.arg("-O3");
        }
        for directory in &options.include_directories {
            command.arg("-I").arg(directory);
        }
        for (name, value) in &options.defines {
            command.arg("-D").arg(format!("{name}={value}"));
        }
        let output = command
            .output()
            .map_err(|error| ShaderCompileError::CompilerUnavailable(error.to_string()))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            cleanup(&[&object_path, &reflection_path, &dump_path]);
            return Err(ShaderCompileError::Compilation {
                status: output.status.code(),
                diagnostics: format!("{stdout}{stderr}"),
            });
        }
        let bytecode_result = std::fs::read(&object_path);
        let binary_result = std::fs::read(&reflection_path);
        let dump_result = std::fs::read_to_string(&dump_path);
        cleanup(&[&object_path, &reflection_path, &dump_path]);
        let bytecode =
            bytecode_result.map_err(|error| ShaderCompileError::ArtifactRead(error.to_string()))?;
        let binary =
            binary_result.map_err(|error| ShaderCompileError::ArtifactRead(error.to_string()))?;
        let dump =
            dump_result.map_err(|error| ShaderCompileError::ArtifactRead(error.to_string()))?;
        if bytecode.is_empty() {
            return Err(ShaderCompileError::EmptyBytecode);
        }
        if binary.is_empty() || dump.is_empty() {
            return Err(ShaderCompileError::MissingReflection);
        }
        #[cfg(target_os = "windows")]
        let bindings = crate::dxc_reflection::reflect_bindings(&binary, &self.executable)
            .map_err(ShaderCompileError::InvalidReflection)?;
        #[cfg(not(target_os = "windows"))]
        let bindings = parse_bindings(&dump)?;
        let mut shader_hasher = blake3::Hasher::new_derive_key("nord.shader.dxil.v1");
        shader_hasher.update(&bytecode);
        let shader_id = crate::ShaderId(*shader_hasher.finalize().as_bytes());
        let diagnostics = if stderr.trim().is_empty() {
            Vec::new()
        } else {
            vec![ShaderDiagnostic { message: stderr, is_error: false }]
        };
        Ok(CompiledShader {
            stage: options.stage,
            shader_id,
            bytecode,
            reflection: ShaderReflection { binary, dump, bindings },
            diagnostics,
        })
    }
}

impl CompiledShader {
    /// Construct a shader from compiler-produced, prebuilt artifacts.
    ///
    /// This is intended for game builds that compile shaders offline and embed
    /// DXIL plus DXC reflection/disassembly artifacts into the executable. It
    /// retains the reflected bindings needed by pipeline creation without
    /// requiring DXC to be installed on the machine running the game.
    ///
    /// # Errors
    /// Returns an error when the bytecode is empty or the reflection dump is
    /// missing or malformed.
    pub fn from_precompiled(
        stage: ShaderStage,
        bytecode: Vec<u8>,
        reflection_binary: Vec<u8>,
        reflection_dump: String,
    ) -> Result<Self, ShaderCompileError> {
        if bytecode.is_empty() {
            return Err(ShaderCompileError::EmptyBytecode);
        }
        if reflection_binary.is_empty() || reflection_dump.is_empty() {
            return Err(ShaderCompileError::MissingReflection);
        }
        let bindings = parse_bindings(&reflection_dump)?;
        let mut shader_hasher = blake3::Hasher::new_derive_key("nord.shader.dxil.v1");
        shader_hasher.update(&bytecode);
        let shader_id = crate::ShaderId(*shader_hasher.finalize().as_bytes());
        Ok(Self {
            stage,
            shader_id,
            bytecode,
            reflection: ShaderReflection {
                binary: reflection_binary,
                dump: reflection_dump,
                bindings,
            },
            diagnostics: Vec::new(),
        })
    }
}

/// Shader compiler failure with source/compiler details retained.
#[derive(Debug)]
pub enum ShaderCompileError {
    SourceNotFound(PathBuf),
    InvalidEntryPoint,
    InvalidTargetProfile,
    CompilerUnavailable(String),
    Compilation { status: Option<i32>, diagnostics: String },
    ArtifactRead(String),
    EmptyBytecode,
    MissingReflection,
    InvalidReflection(String),
}
impl std::fmt::Display for ShaderCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceNotFound(path) => {
                write!(f, "shader source does not exist: {}", path.display())
            }
            Self::InvalidEntryPoint => {
                f.write_str("shader entry point must not be empty or contain whitespace")
            }
            Self::InvalidTargetProfile => {
                f.write_str("shader target profile is not supported by the selected stage")
            }
            Self::CompilerUnavailable(message) => write!(f, "could not start DXC: {message}"),
            Self::Compilation { status, diagnostics } => {
                write!(f, "DXC compilation failed (status {status:?}): {diagnostics}")
            }
            Self::ArtifactRead(message) => write!(f, "could not read DXC output: {message}"),
            Self::EmptyBytecode => f.write_str("DXC succeeded but emitted empty shader bytecode"),
            Self::MissingReflection => {
                f.write_str("DXC succeeded but emitted no shader reflection artifacts")
            }
            Self::InvalidReflection(message) => {
                write!(f, "DXC reflection output is invalid: {message}")
            }
        }
    }
}
impl std::error::Error for ShaderCompileError {}

fn validate_options(options: &ShaderCompileOptions) -> Result<(), ShaderCompileError> {
    if options.entry_point.trim().is_empty() || options.entry_point.chars().any(char::is_whitespace)
    {
        return Err(ShaderCompileError::InvalidEntryPoint);
    }
    let valid = match options.stage {
        ShaderStage::Vertex => options.target_profile.starts_with("vs_"),
        ShaderStage::Pixel => options.target_profile.starts_with("ps_"),
        ShaderStage::Compute => options.target_profile.starts_with("cs_"),
    };
    if !valid {
        return Err(ShaderCompileError::InvalidTargetProfile);
    }
    Ok(())
}

fn parse_bindings(dump: &str) -> Result<Vec<ShaderResourceBinding>, ShaderCompileError> {
    let mut in_table = false;
    let mut bindings = Vec::new();
    let mut found_table = false;
    for line in dump.lines() {
        if line.contains("Resource Bindings:") {
            in_table = true;
            found_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let line = line.trim().trim_start_matches(';').trim();
        if line.ends_with(':') && !line.contains("Name ") {
            // Resource-free stages have an empty table; stop at DXC's next
            // disassembly section instead of treating its rows as bindings.
            break;
        }
        if line.is_empty() {
            // DXC emits comment-only separator rows before the table header.
            // Keep scanning until at least one binding has been parsed; after
            // that, the blank separator marks the end of the table.
            if bindings.is_empty() {
                continue;
            }
            break;
        }
        if line.starts_with("Name ") || line.starts_with("---") {
            continue;
        }
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 7 {
            continue;
        }
        let hlsl_register = columns[5];
        let (register_token, space) =
            hlsl_register.split_once(',').map_or((hlsl_register, 0), |(register, space)| {
                (
                    register,
                    space.strip_prefix("space").and_then(|value| value.parse().ok()).unwrap_or(0),
                )
            });
        let Some(prefix) = register_token.chars().next() else {
            return Err(ShaderCompileError::InvalidReflection(format!(
                "binding `{}` has no register namespace",
                columns[0]
            )));
        };
        let kind = match prefix {
            'b' | 'c' => ShaderBindingKind::ConstantBuffer,
            't' => ShaderBindingKind::SampledResource,
            'u' => ShaderBindingKind::StorageResource,
            's' => ShaderBindingKind::Sampler,
            _ => {
                return Err(ShaderCompileError::InvalidReflection(format!(
                    "binding `{}` uses unsupported register `{register_token}`",
                    columns[0]
                )));
            }
        };
        let Some(register) = register_token
            .trim_start_matches(|character: char| !character.is_ascii_digit())
            .parse()
            .ok()
        else {
            return Err(ShaderCompileError::InvalidReflection(format!(
                "binding `{}` has an invalid register `{register_token}`",
                columns[0]
            )));
        };
        let Some(count) = columns[6].parse().ok().filter(|count| *count > 0) else {
            return Err(ShaderCompileError::InvalidReflection(format!(
                "binding `{}` has an invalid descriptor count",
                columns[0]
            )));
        };
        bindings.push(ShaderResourceBinding {
            name: columns[0].to_owned(),
            kind,
            register,
            space,
            count,
        });
    }
    if !found_table {
        return Err(ShaderCompileError::MissingReflection);
    }
    Ok(bindings)
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(target_os = "windows")]
fn find_windows_sdk_dxc() -> Option<OsString> {
    let program_files = std::env::var_os("ProgramFiles(x86)")?;
    let bin = PathBuf::from(program_files).join("Windows Kits").join("10").join("bin");
    let mut versions = std::fs::read_dir(bin)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort_unstable_by(|left, right| right.file_name().cmp(&left.file_name()));
    versions
        .into_iter()
        .map(|version| version.join("x64").join("dxc.exe"))
        .find(|candidate| candidate.is_file())
        .map(PathBuf::into_os_string)
}

#[cfg(test)]
mod tests {
    use super::{ShaderBindingKind, parse_bindings};

    #[test]
    fn parses_dxc_resource_table_after_comment_separators() {
        let dump = "\
; Resource Bindings:
;
; Name                                 Type  Format         Dim      ID      HLSL Bind  Count
; ------------------------------ ---------- ------- ----------- ------- -------------- ------
; CameraData                        cbuffer      NA          NA     CB0            cb0     1
;
; ViewId state:
";

        let bindings = parse_bindings(dump).expect("DXC resource table should parse");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].name, "CameraData");
        assert_eq!(bindings[0].kind, ShaderBindingKind::ConstantBuffer);
        assert_eq!(bindings[0].register, 0);
        assert_eq!(bindings[0].space, 0);
        assert_eq!(bindings[0].count, 1);
    }

    #[test]
    fn accepts_empty_dxc_resource_table_without_parsing_following_sections() {
        let dump = "\
; Resource Bindings:
;
; Name                                 Type  Format         Dim      ID      HLSL Bind  Count
; ------------------------------ ---------- ------- ----------- ------- -------------- ------
;
; ViewId state:
;
; Number of inputs: 4, outputs: 1
;   output 0 depends on inputs: { 2 }
";

        assert!(parse_bindings(dump).expect("empty table should be valid").is_empty());
    }
}

#[cfg(not(target_os = "windows"))]
fn find_windows_sdk_dxc() -> Option<OsString> {
    None
}
