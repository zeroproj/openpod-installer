//! Permissão de administrador no macOS.
//!
//! A janela continua rodando como usuário comum; só a parte do aparelho
//! roda elevada. Quando o macOS prende o player (acesso negado, driver do
//! kernel ativo ou aparelho ocupado), a janela chama o nosso próprio
//! binário no modo `--trabalhador` com `osascript ... with administrator
//! privileges`: o macOS pede a senha do usuário, uma vez por sessão.

use crate::aparelho::canal::Canal;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// Por que o pedido não deu certo.
pub enum Erro {
    /// o usuário cancelou o pedido de senha
    Senha,
    /// outra coisa deu errado
    Outra(String),
}

/// O estado do pedido de senha.
pub struct Elevacao {
    ativo: bool,
    pendente: bool,
    rx: Option<Receiver<Result<Canal, Erro>>>,
}

impl Elevacao {
    pub fn novo() -> Elevacao {
        Elevacao { ativo: false, pendente: false, rx: None }
    }

    /// O erro diz que o macOS está prendendo o aparelho?
    pub fn precisa(&self, e: &str) -> bool {
        imp::precisa(e)
    }

    /// O trabalhador elevado já está de pé?
    pub fn ativo(&self) -> bool {
        self.ativo
    }

    /// O macOS está mostrando o pedido de senha agora?
    pub fn pendente(&self) -> bool {
        self.pendente
    }

    /// Pede a senha. Só um pedido por sessão: enquanto o pedido está na
    /// tela (ou o trabalhador já de pé) não se pede de novo.
    pub fn pedir(&mut self) {
        if self.pendente || self.ativo {
            return;
        }
        self.pendente = true;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(imp::iniciar());
        });
    }

    /// O resultado do pedido, se já tiver vindo.
    pub fn resultado(&mut self) -> Option<Result<Canal, Erro>> {
        let rx = self.rx.take()?;
        match rx.try_recv() {
            Ok(r) => {
                self.pendente = false;
                if r.is_ok() {
                    self.ativo = true;
                }
                Some(r)
            }
            Err(TryRecvError::Empty) => {
                self.rx = Some(rx);
                None
            }
            Err(TryRecvError::Disconnected) => {
                self.pendente = false;
                None
            }
        }
    }
}

/// macOS: pede a senha e reabre o instalador inteiro como administrador.
#[cfg(target_os = "macos")]
pub fn reabrir_como_admin_publico() -> Result<(), Erro> {
    imp::reabrir_como_admin()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use crate::aparelho::canal;
    use crate::err;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// O macOS prende o aparelho: acesso negado (E212) ou aparelho ocupado
    /// (E213). O detach do driver do kernel também cai aqui.
    pub fn precisa(e: &str) -> bool {
        e.starts_with("E212") || e.starts_with("E213")
    }

    /// A pasta do socket: `~/Library/Caches/OpenPod/`, só do usuário.
    fn pasta() -> Result<PathBuf, String> {
        let base = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| err!(E732, "não achei a pasta do usuário"))?;
        let dir = base.join("Library").join("Caches").join("OpenPod");
        std::fs::create_dir_all(&dir).map_err(|e| err!(E701, "a pasta {}: {e}", dir.display()))?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| err!(E727, "as permissões de {}: {e}", dir.display()))?;
        Ok(dir)
    }

    /// Um texto para dentro de aspas do AppleScript.
    fn apple(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Uma palavra para o shell.
    fn sh(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    /// O comando de shell que roda o nosso binário como trabalhador.
    fn comando(binario: &Path, socket: &Path, token: &str, uid: u32, gid: u32) -> String {
        format!("{} --trabalhador {} {} {} {}", sh(&binario.to_string_lossy()), sh(&socket.to_string_lossy()), sh(token), uid, gid)
    }

    /// O AppleScript que roda o nosso binário como administrador.
    fn applescript(binario: &Path, socket: &Path, token: &str, uid: u32, gid: u32) -> String {
        // o diario do trabalhador vai ao lado do socket (pasta 0700 do usuario), para diagnostico
        let diario = socket.with_file_name("trabalhador.log");
        let comando = format!("{} >{} 2>&1 &", comando(binario, socket, token, uid, gid), sh(&diario.to_string_lossy()));
        format!("do shell script \"{}\" with administrator privileges", apple(&comando))
    }

    /// Reabre o instalador inteiro como administrador (como o `sudo` que
    /// funcionou no aparelho) e fecha esta janela. O trabalhador separado
    /// (abaixo) deixou o macOS recusar o claim (E216) depois do detach.
    pub fn iniciar() -> Result<Canal, Erro> {
        reabrir_como_admin().map(|_| unreachable!())
    }

    /// Pede a senha ao macOS e reabre o instalador inteiro como
    /// administrador; em caso de sucesso este processo termina aqui.
    pub fn reabrir_como_admin() -> Result<(), Erro> {
        // O que funciona no aparelho e o `sudo` de verdade (CONFIRMADO em 2026-11-22: a mesma versao
        // conecta com `sudo` no Terminal e da E216 pelo osascript "with administrator privileges",
        // mesmo com `launchctl asuser`). Entao: `sudo -A` com uma caixa de senha nossa (askpass).
        let dir = pasta().map_err(Erro::Outra)?;
        let binario = std::env::current_exe().map_err(|e| Erro::Outra(err!(E736, "o caminho do instalador (admin): {e}")))?;
        let (uid, gid) = (unsafe { libc::getuid() }, unsafe { libc::getgid() });
        let pt = crate::textos::Idioma::detectar() == crate::textos::Idioma::Pt;
        let (msg, titulo) = if pt {
            ("O OpenPod Tune precisa da sua senha do Mac para falar com o player.", "OpenPod Tune")
        } else {
            ("OpenPod Tune needs your Mac password to talk to the player.", "OpenPod Tune")
        };
        let askpass = dir.join("senha.sh");
        let corpo = format!(
            "#!/bin/sh\nexec /usr/bin/osascript -e 'text returned of (display dialog \"{}\" default answer \"\" with hidden answer with title \"{}\" with icon caution)'\n",
            msg, titulo
        );
        std::fs::write(&askpass, corpo).map_err(|e| Erro::Outra(err!(E738, "a caixa de senha: {e}")))?;
        std::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| Erro::Outra(err!(E739, "a caixa de senha (permissao): {e}")))?;
        // Uma chamada so: o sudo pede a senha pela caixa (askpass) e ja abre o instalador
        // como root. (Em duas chamadas, `sudo -v` + `sudo -n`, o macOS nao aproveita a senha
        // sem terminal: "a password is required", CONFIRMADO em 2026-11-22.)
        let marca = dir.join("admin.ok");
        let _ = std::fs::remove_file(&marca);
        let diario = std::fs::File::create(dir.join("admin.log")).ok();
        let mut cmd = Command::new("/usr/bin/sudo");
        cmd.arg("-A").arg(&binario).arg("--admin").arg(uid.to_string()).arg(gid.to_string());
        cmd.env("SUDO_ASKPASS", &askpass).stdin(Stdio::null());
        if let Some(f) = diario {
            if let Ok(f2) = f.try_clone() {
                cmd.stdout(f).stderr(f2);
            }
        }
        let mut filho = cmd.spawn().map_err(|e| Erro::Outra(err!(E737, "o sudo nao iniciou: {e}")))?;
        // espera a janela nova avisar que subiu (admin.ok) ou o sudo sair (senha recusada)
        let t0 = std::time::Instant::now();
        loop {
            if marca.exists() {
                let _ = std::fs::remove_file(&marca);
                let _ = std::fs::remove_file(&askpass);
                crate::log::linha("ADMIN", "reaberto com sudo; fechando esta janela");
                std::process::exit(0);
            }
            if let Ok(Some(st)) = filho.try_wait() {
                let _ = std::fs::remove_file(&askpass);
                crate::log::linha("ADMIN", &format!("o sudo saiu sem abrir o instalador ({st})"));
                return Err(Erro::Senha);
            }
            if t0.elapsed() > Duration::from_secs(300) {
                let _ = std::fs::remove_file(&askpass);
                return Err(Erro::Outra(err!(E735, "o macOS nao reabriu o instalador a tempo")));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    #[allow(dead_code)]
    pub fn iniciar_trabalhador() -> Result<Canal, Erro> {
        let dir = pasta().map_err(Erro::Outra)?;
        let socket = dir.join("trabalhador.sock");
        // socket de uma sessão antiga
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).map_err(|e| Erro::Outra(err!(E729, "o socket {}: {e}", socket.display())))?;
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
        let token = canal::token().map_err(Erro::Outra)?;
        let binario = std::env::current_exe().map_err(|e| Erro::Outra(err!(E728, "o caminho do instalador: {e}")))?;
        let (uid, gid) = (unsafe { libc::getuid() }, unsafe { libc::getgid() });

        let script = applescript(&binario, &socket, &token, uid, gid);
        let child = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Erro::Outra(err!(E726, "o osascript não iniciou: {e}")))?;

        // o osascript sai quando a senha é recusada (ou quando o
        // trabalhador começa a falar com a gente)
        let saida = Arc::new(Mutex::new(None));
        let destino = saida.clone();
        std::thread::spawn(move || {
            if let Ok(o) = child.wait_with_output() {
                *destino.lock().unwrap() = Some(o);
            }
        });

        let recusou = {
            let saida = saida.clone();
            move || match saida.lock().unwrap().as_ref() {
                Some(o) => !o.status.success(),
                None => false,
            }
        };

        match canal::aceitar_com(&listener, &token, Duration::from_secs(180), &recusou) {
            Ok(con) => Ok(Canal::nova(con)),
            Err(e) => {
                let cancelou = saida
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|o| !o.status.success() && String::from_utf8_lossy(&o.stderr).contains("User canceled"));
                match cancelou {
                    Some(true) => Err(Erro::Senha),
                    _ => Err(Erro::Outra(e)),
                }
            }
        }
    }

    #[cfg(test)]
    mod testes {
        use super::*;
        use std::path::Path;

        /// O caminho do instalador com espaço e aspas: o comando tem de
        /// chegar inteiro no shell.
        #[test]
        fn o_comando_chega_inteiro_no_shell() {
            let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("elev-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let args = dir.join("args.txt");
            let falso = dir.join("falso instalador");
            std::fs::write(&falso, format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\n", args.display()))
                .unwrap();
            let _ = std::fs::set_permissions(&falso, std::os::unix::fs::PermissionsExt::from_mode(0o755));

            let binario = Path::new(&falso);
            let socket = Path::new("/Users/um/usuário/Library/Caches/OpenPod/trabalhador.sock");
            let token = "ab\"cd'ef";
            let linha = comando(binario, socket, token, 501, 20);
            let r = std::process::Command::new("/bin/sh").arg("-c").arg(&linha).status().unwrap();
            assert!(r.success(), "o comando não rodou: {linha}");

            let lido = std::fs::read_to_string(&args).unwrap();
            let partes: Vec<&str> = lido.lines().collect();
            assert_eq!(partes[0], "--trabalhador", "{partes:?}");
            assert_eq!(partes[1], socket.to_string_lossy(), "{partes:?}");
            assert_eq!(partes[2], token, "{partes:?}");
            assert_eq!(partes[3], "501", "{partes:?}");
            assert_eq!(partes[4], "20", "{partes:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// O AppleScript tem que compilar (o osacompile só confere a
        /// sintaxe, não roda nada).
        #[test]
        fn o_applescript_compila() {
            let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("elev-apple-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let script = applescript(
                Path::new("/Applications/OpenPod Tune.app/Contents/MacOS/openpod-tune"),
                Path::new("/Users/um/Library/Caches/OpenPod/trabalhador.sock"),
                "a".repeat(64).as_str(),
                501,
                20,
            );
            let saida =
                std::process::Command::new("/usr/bin/osacompile").arg("-o").arg(dir.join("teste.scpt")).arg("-e").arg(&script).output();
            match saida {
                Ok(o) => assert!(o.status.success(), "o applescript não compila: {script}\n{}", String::from_utf8_lossy(&o.stderr)),
                Err(e) => panic!("o osacompile não rodou: {e}"),
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
