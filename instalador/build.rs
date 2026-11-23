//! Windows: a versão, o ícone e o manifesto entram no .exe (build.rs).
//!
//! Um .exe sem versão e sem ícone é mais uma coisa que o antivírus conta
//! contra ele. O manifesto diz que ele roda como o usuário que o abriu: o
//! SCSI pass-through pelo driver de disco do próprio Windows abre o volume
//! do player como usuário comum (só o disco inteiro pediria administrador),
//! e o cliente não instala driver nenhum.
//!
//! O recurso é compilado pelo `windres` do MinGW do alvo e entra no link do
//! .exe como um objeto comum.
//!
//! Adaptado do rockslop de Jayden Milne (GPL-2.0-or-later), `build.rs`.
//! Veja CREDITOS.md.

use std::path::Path;
use std::process::Command;

/// A versão do Windows são quatro números de 16 bits: "0.1.0" vira
/// "0,1,0,0".
fn numeros(versao: &str) -> String {
    let partes: Vec<u16> = versao.split('.').map(|p| p.parse::<u16>().unwrap_or(0)).collect();
    let n = |i: usize| partes.get(i).copied().unwrap_or(0);
    format!("{},{},{},{}", n(0), n(1), n(2), n(3))
}

/// O `windres` do MinGW do alvo (`x86_64-w64-mingw32-windres`).
fn windres() -> String {
    if let Ok(w) = std::env::var("OPENPOD_WINDRES") {
        return w;
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    let prefix = match target.strip_suffix("-pc-windows-gnu") {
        Some(arch) => format!("{arch}-w64-mingw32"),
        None => "x86_64-w64-mingw32".to_string(),
    };
    format!("{prefix}-windres")
}

/// O recurso do .exe: ícone, manifesto e versão.
fn recursos(root: &Path) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icone = root.join("assets").join("OpenIcon.ico");
    let manifesto = root.join("packaging").join("windows").join("openpod.manifest");
    for f in [&icone, &manifesto] {
        println!("cargo:rerun-if-changed={}", f.display());
    }

    let versao = env!("CARGO_PKG_VERSION");
    let v = numeros(versao);
    // números, e não os nomes de winver.h, para não precisar do cabeçalho:
    // 24 RT_MANIFEST, 0x40004 VOS_NT_WINDOWS32, 1 VFT_APP
    let fwd = |p: &Path| p.display().to_string().replace('\\', "/");
    let bin = "openpod-tune";
    let rc = format!(
        "1 ICON \"{icone}\"\n1 24 \"{manifesto}\"\n\
         1 VERSIONINFO\nFILEVERSION {v}\nPRODUCTVERSION {v}\nFILEFLAGSMASK 0x3f\nFILEFLAGS 0\n\
         FILEOS 0x40004\nFILETYPE 1\nFILESUBTYPE 0\nBEGIN\n\
         BLOCK \"StringFileInfo\"\nBEGIN\nBLOCK \"040904b0\"\nBEGIN\n\
         VALUE \"CompanyName\", \"OpenPod\"\n\
         VALUE \"FileDescription\", \"OpenPod Tune\"\n\
         VALUE \"FileVersion\", \"{versao}\"\n\
         VALUE \"InternalName\", \"{bin}\"\n\
         VALUE \"LegalCopyright\", \"GPL-2.0-or-later\"\n\
         VALUE \"OriginalFilename\", \"{bin}.exe\"\n\
         VALUE \"ProductName\", \"OpenPod Tune\"\n\
         VALUE \"ProductVersion\", \"{versao}\"\n\
         END\nEND\n\
         BLOCK \"VarFileInfo\"\nBEGIN\nVALUE \"Translation\", 0x409, 1200\nEND\nEND\n",
        icone = fwd(&icone),
        manifesto = fwd(&manifesto),
    );

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let caminho_rc = out.join(format!("{bin}.rc"));
    std::fs::write(&caminho_rc, rc).unwrap();
    let objeto = out.join(format!("{bin}-recurso.o"));

    let windres = windres();
    let saida = Command::new(&windres)
        .args(["-O", "coff", "-i"])
        .arg(&caminho_rc)
        .args(["-o"])
        .arg(&objeto)
        .output()
        .unwrap_or_else(|e| panic!("o {windres} não rodou: {e}"));
    if !saida.status.success() {
        panic!("o {windres} falhou: {}", String::from_utf8_lossy(&saida.stderr));
    }
    // o objeto do recurso entra no link do .exe
    println!("cargo:rustc-link-arg-bins={}", objeto.display());
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    recursos(root);
}
