//! Instalador OpenPod: OpenPod System ou OpenPod RockBox no player GN-438.
//! GPL-2.0-or-later. Veja CREDITOS.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aparelho;
mod app;
#[cfg(target_os = "macos")]
mod elevacao;
mod estilo;
mod log;
mod pacote;
mod site;
mod textos;
#[cfg(unix)]
mod trabalhador;

fn uso() {
    eprintln!("uso: openpod-instalador");
    #[cfg(unix)]
    eprintln!("     openpod-instalador --trabalhador <socket> <token> <uid> <gid>");
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // O modo trabalhador: o mesmo binário, sem janela, falando com o
    // aparelho com a permissão que o macOS deu.
    #[cfg(unix)]
    if args.get(1).map(|a| a == "--trabalhador").unwrap_or(false) {
        std::process::exit(trabalhador::rodar(&args[2..]));
    }

    // macOS (decisor): ja ao abrir, pede a senha e sobe o instalador inteiro
    // como administrador, como um `sudo` desde o comeco. Se a senha for
    // recusada, abre normal (e pede de novo quando precisar).
    #[cfg(target_os = "macos")]
    if unsafe { libc::geteuid() } != 0 && args.get(1).map(|a| a != "--admin" && a != "--trabalhador").unwrap_or(true) {
        if let Err(e) = elevacao::reabrir_como_admin_publico() {
            let _ = e;
            eprintln!("[ADMIN] sem administrador; abrindo normal");
        }
    }

    // O modo administrador (macOS): o app inteiro reaberto como root, como o
    // `sudo` que funcionou no aparelho. A casa e o dono continuam os do usuario.
    #[cfg(unix)]
    let args: Vec<String> = if args.get(1).map(|a| a == "--admin").unwrap_or(false) {
        let uid: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let gid: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        if let Some(casa) = trabalhador::casa_do_usuario(uid) {
            std::env::set_var("HOME", casa);
        }
        std::env::set_var("OPENPOD_DONO", format!("{uid}:{gid}"));
        // avisa a janela que pediu a senha que esta subiu (ela fecha ao ver a marca)
        if let Some(casa) = std::env::var_os("HOME") {
            let _ = std::fs::write(std::path::PathBuf::from(casa).join("Library/Caches/OpenPod/admin.ok"), b"ok");
        }
        args[..1].to_vec()
    } else {
        args
    };

    if let Some(arg) = args.get(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                uso();
                return Ok(());
            }
            outro => {
                eprintln!("argumento desconhecido: {outro}");
                uso();
                std::process::exit(2);
            }
        }
    }

    let idioma = textos::Idioma::detectar();
    let titulo = textos::t(idioma, "titulo_app");
    let opcoes = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(titulo)
            .with_inner_size([980.0, 780.0])
            .with_min_inner_size([900.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(titulo, opcoes, Box::new(|cc| Ok(Box::new(app::App::novo(cc)))))
}
