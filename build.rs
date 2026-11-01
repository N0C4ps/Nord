use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SHADER_SOURCE: &str = "assets/shaders/nord_pbr_forward.hlsl";

fn main() {
    println!("cargo:rerun-if-changed=nord.exe.manifest");
    println!("cargo:rerun-if-changed={SHADER_SOURCE}");
    for variable in ["NORD_DXC_PATH", "DXC_PATH", "ProgramFiles(x86)"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package directory"),
        )
        .join("nord.exe.manifest");
        println!("cargo:rustc-link-arg-bin=nord=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=nord=/MANIFESTINPUT:{}", manifest.display());
    }

    compile_game_shaders();
}

fn compile_game_shaders() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package directory"));
    let shader_path = manifest_dir.join(SHADER_SOURCE);
    let shader_directory =
        shader_path.parent().expect("the checked-in shader path has a parent directory");
    let output_directory =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts"));
    fs::create_dir_all(&output_directory).expect("create Cargo's shader artifact directory");
    let dxc = find_dxc().unwrap_or_else(|| {
        panic!(
            "DXC is required to build Nord's HLSL assets. Install the Windows SDK or set NORD_DXC_PATH. DXC is a build tool; the game executable embeds its shader artifacts."
        )
    });

    compile_stage(
        &dxc,
        &shader_path,
        shader_directory,
        &output_directory,
        "VSMain",
        "vs_6_0",
        "vs",
    );
    compile_stage(
        &dxc,
        &shader_path,
        shader_directory,
        &output_directory,
        "PSMain",
        "ps_6_0",
        "ps",
    );
}

fn compile_stage(
    dxc: &Path,
    shader: &Path,
    include_directory: &Path,
    output_directory: &Path,
    entry: &str,
    profile: &str,
    stage: &str,
) {
    let bytecode = output_directory.join(format!("nord_pbr_forward.{stage}.dxil"));
    let reflection = output_directory.join(format!("nord_pbr_forward.{stage}.reflection"));
    let disassembly = output_directory.join(format!("nord_pbr_forward.{stage}.asm"));
    let mut command = Command::new(dxc);
    command
        .arg(shader)
        .args(["-E", entry, "-T", profile, "-Fo"])
        .arg(&bytecode)
        .arg("-Fre")
        .arg(&reflection)
        .arg("-Fc")
        .arg(&disassembly)
        .arg("-I")
        .arg(include_directory);
    if env::var("PROFILE").as_deref() == Ok("debug") {
        command.args(["-Zi", "-Qembed_debug"]);
    } else {
        command.arg("-O3");
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("could not start DXC at {}: {error}", dxc.display()));
    assert!(
        output.status.success(),
        "DXC failed compiling {stage} shader (status {:?}):\n{}{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.stderr.is_empty() {
        println!(
            "cargo:warning=DXC {stage} shader diagnostics: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

fn find_dxc() -> Option<PathBuf> {
    if let Some(path) = env::var_os("NORD_DXC_PATH").or_else(|| env::var_os("DXC_PATH")) {
        return Some(PathBuf::from(path));
    }
    find_windows_sdk_dxc().or_else(|| find_on_path("dxc.exe").or_else(|| find_on_path("dxc")))
}

fn find_on_path(executable: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(executable))
        .find(|candidate| candidate.is_file())
}

fn find_windows_sdk_dxc() -> Option<PathBuf> {
    let program_files = env::var_os("ProgramFiles(x86)")?;
    let bin = PathBuf::from(program_files).join("Windows Kits").join("10").join("bin");
    let mut versions = fs::read_dir(bin)
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
}
