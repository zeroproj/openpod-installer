//! O aparelho: presença no USB, o trabalho (backup, conferência, gravação,
//! cartão) numa thread, e os eventos que ele manda para a interface.
//!
//! A camada do aparelho vem do rockslop de Jayden Milne (GPL-2.0-or-later),
//! adaptada para o instalador do OpenPod; veja CREDITOS.md.

#![allow(dead_code)]

pub mod backup;
pub mod bot;
#[cfg(unix)]
pub mod canal;
pub mod cartao;
pub mod dono;
pub mod limpa;
pub mod real;
pub mod regras;
pub mod sdcard;
#[cfg(test)]
pub mod sim;
/// Windows: o SCSI pass-through pelo driver de disco do próprio Windows
/// (sem WinUSB, sem Zadig).
#[cfg(windows)]
pub mod spt;
pub mod usb;
pub mod yp3;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// Uma mensagem de erro com o número na frente ("E270: bateria baixa ..."),
/// para a tradução achar o texto. O primeiro argumento é o número (`E` e
/// três dígitos); o resto é `format!`. Cada número aparece uma vez só.
///
/// 1xx pacote, 2xx USB / modo de download / identidade / bateria, 3xx
/// gravação e checksum, 4xx backup e restauração, 5xx cartão, 9xx interno.
#[macro_export]
macro_rules! err {
    ($code:ident, $($fmt:tt)+) => {
        format!("{}: {}", stringify!($code), format_args!($($fmt)+))
    };
}

pub const FLASH: u32 = 2 * 1024 * 1024;
pub const SETOR: u32 = 4096;
/// Nunca abaixo disto, nunca a partir dali: o bootloader fica embaixo, a
/// tabela de partições em 0xD000, e os ajustes de fábrica em cima.
pub const GRAVAVEL: (u32, u32) = (0xE000, 0x1FC000);

/// O bootloader de 0x0 a 0xD000 (a conferência antes de gravar).
pub const BOOT_SHA256: &str = "47fe9ad6074d83b8d2a0619bdfb66a021bd94ba4e86abdfaeda574952ed038af";
/// A tabela de partições, 0xD000 + 0x1C.
pub const TABELA_GN438: &str = "ca23eb0ca75fbaa89fd8fbabe086e5a4d1ad8e6750e9f273d96eae092b9e9983";
/// O player do autor do Rockbox YP3: aceito só pelo OpenPod RockBox.
pub const TABELA_AUTOR: &str = "3b8c6abe37c52ede4ade536275ee0cb772b68861ed78b27246c7728fefc66661";

/// O que vemos no barramento.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Modo {
    /// ROM de recuperação (301a:2800): pronto para gravar
    Rom,
    Stock,
    Rockbox,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Visto {
    pub modo: Modo,
    /// false: presente, mas este programa não consegue abrir (driver)
    pub acesso: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Fase {
    Connect,
    Backup,
    Verify,
    Write,
    Check,
    Reboot,
    Sync,
    Eject,
    Done,
}

impl Fase {
    pub fn rotulo(self) -> &'static str {
        match self {
            Fase::Connect => "connect",
            Fase::Backup => "backup",
            Fase::Verify => "verify",
            Fase::Write => "write",
            Fase::Check => "check",
            Fase::Reboot => "reboot",
            Fase::Sync => "sync",
            Fase::Eject => "eject",
            Fase::Done => "done",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Nivel {
    Info,
    Ok,
    Warn,
    Err,
    Dim,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resultado {
    pub backup: Option<PathBuf>,
    pub arquivos: usize,
    pub bytes: u64,
    pub cartao: bool,
    pub restaurado: bool,
    /// o cartão já tinha `config.cfg`: foi preservado (só o tema e o idioma mudaram)
    pub config_mantida: bool,
    /// a pasta `.rockbox` do cartão foi apagada: o Rockbox começou do zero
    pub comecou_do_zero: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Evento {
    Presenca(Vec<Visto>),
    Log(Nivel, String),
    /// conferência: chave, valor, veredito
    Conferencia(String, String, Option<bool>),
    Fase(Fase),
    Progresso(f32),
    /// um arquivo do cartão sendo escrito: caminho e tamanho
    Arquivo(String, u64),
    /// o cartão não está lá (ou não tem cartão)
    PrecisaCartao(bool),
    /// o Rockbox está de pé mas não mostra disco USB
    PrecisaDisco(bool),
    /// a bateria em mV, lida no player
    Bateria(u32),
    /// a cópia de segurança já está em disco (para a recuperação)
    Copia(PathBuf),
    Pronto,
    Feito(Resultado),
    Falhou(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Comando {
    /// de quem a interface está esperando (o simulador usa para encenar)
    Assistir(bool),
    /// procura o player e diz o que viu (sem tocar em nada)
    Procurar,
    /// só identificar o player e ler a bateria, sem gravar nada
    Conferir,
    Gravar {
        pacote: PathBuf,
        idioma: crate::textos::Idioma,
        /// O arquivo de idioma do RockBox escolhido pelo usuário (o nome do
        /// `.lng`, como `english.lng`). `None` = o padrão (o idioma do
        /// instalador). O `serde(default)` deixa a linha antiga do canal
        /// (sem o campo) ser lida sem erro.
        #[serde(default)]
        lang_rockbox: Option<String>,
        /// Apagar a pasta `.rockbox` do cartão antes de instalar (o Rockbox
        /// começa do zero). O `serde(default)` mantém a compatibilidade do
        /// canal JSON.
        #[serde(default)]
        limpar_rockbox: bool,
    },
    Restaurar {
        imagem: PathBuf,
    },
}

/// Quem sabe achar e comandar o aparelho: o USB de verdade, ou a demo.
pub trait Backend: Send {
    fn scan(&mut self, ev: &Sink) -> Vec<Visto>;
    fn run(&mut self, cmd: Comando, ev: &Sink) -> Result<(), String>;
}
/// O remetente de eventos, com atalhos.
#[derive(Clone)]
pub struct Sink(pub Sender<Evento>);

impl Sink {
    pub fn send(&self, e: Evento) {
        crate::log::evento(&e);
        let _ = self.0.send(e);
    }
    pub fn log(&self, n: Nivel, s: impl Into<String>) {
        self.send(Evento::Log(n, s.into()));
    }
    pub fn info(&self, s: impl Into<String>) {
        self.log(Nivel::Info, s)
    }
    pub fn ok(&self, s: impl Into<String>) {
        self.log(Nivel::Ok, s)
    }
    pub fn aviso(&self, s: impl Into<String>) {
        self.log(Nivel::Warn, s)
    }
    pub fn dim(&self, s: impl Into<String>) {
        self.log(Nivel::Dim, s)
    }
    pub fn fase(&self, f: Fase) {
        self.send(Evento::Fase(f))
    }
    pub fn progresso(&self, f: f32) {
        self.send(Evento::Progresso(f.clamp(0.0, 1.0)))
    }
    pub fn conferencia(&self, k: &str, v: impl Into<String>, ok: Option<bool>) {
        self.send(Evento::Conferencia(k.to_string(), v.into(), ok))
    }
}

/// A troca de backend: a janela começa com o USB direto e, no macOS, troca
/// pelo trabalhador elevado quando o macOS pede a senha.
struct Troca {
    backend: Box<dyn Backend>,
    aviso: Sender<Result<(), String>>,
}

pub struct Hub {
    rx: Receiver<Evento>,
    tx: Sender<Comando>,
    trocas: Sender<Troca>,
    pub vistos: Vec<Visto>,
    pub ocupado: bool,
}

impl Hub {
    pub fn iniciar(mut backend: Box<dyn Backend>) -> Hub {
        let (etx, erx) = channel();
        let (ctx, crx) = channel::<Comando>();
        let (ttx, trx) = channel::<Troca>();
        std::thread::spawn(move || {
            let sink = Sink(etx);
            let mut ultimo: Option<Vec<Visto>> = None;
            loop {
                // A troca vem antes de tudo: o comando que vem logo depois
                // já sai pelo backend novo.
                while let Ok(t) = trx.try_recv() {
                    backend = t.backend;
                    let _ = t.aviso.send(Ok(()));
                }
                match crx.recv_timeout(Duration::from_millis(250)) {
                    Ok(cmd) => {
                        let trabalho = !matches!(cmd, Comando::Assistir(_) | Comando::Procurar);
                        if trabalho {
                            crate::log::linha("CMD", &format!("{cmd:?}"));
                        }
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.run(cmd, &sink)));
                        match r {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => sink.send(Evento::Falhou(e)),
                            Err(p) => {
                                let msg = p
                                    .downcast_ref::<String>()
                                    .cloned()
                                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                                    .unwrap_or_else(|| "internal error".into());
                                sink.send(Evento::Falhou(err!(E901, "erro interno: {msg}")));
                            }
                        }
                        if trabalho {
                            ultimo = None;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Um pânico no scan (ex.: libusb indisponível) não pode
                        // matar a thread e deixar a interface presa.
                        let agora = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.scan(&sink))) {
                            Ok(v) => v,
                            Err(_) => {
                                crate::log::linha("SCAN", "pânico no scan; seguindo");
                                Vec::new()
                            }
                        };
                        if ultimo.as_ref() != Some(&agora) {
                            sink.send(Evento::Presenca(agora.clone()));
                            ultimo = Some(agora);
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        Hub { rx: erx, tx: ctx, trocas: ttx, vistos: Vec::new(), ocupado: false }
    }

    pub fn enviar(&mut self, c: Comando) {
        if !matches!(c, Comando::Assistir(_) | Comando::Procurar) {
            self.ocupado = true;
        }
        let _ = self.tx.send(c);
    }

    /// Troca o backend (o macOS: do USB direto para o trabalhador elevado).
    /// O aviso chega quando a troca foi feita: só então o comando pode ser
    /// reenviado.
    pub fn trocar_backend(&mut self, novo: Box<dyn Backend>) -> Receiver<Result<(), String>> {
        let (atx, arx) = channel();
        let _ = self.trocas.send(Troca { backend: novo, aviso: atx });
        arx
    }

    /// Esvazia os eventos pendentes (a presença fica em `vistos`).
    pub fn poll(&mut self) -> Vec<Evento> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            match &e {
                Evento::Presenca(s) => self.vistos = s.clone(),
                Evento::Feito(_) | Evento::Falhou(_) | Evento::Pronto => self.ocupado = false,
                _ => {}
            }
            out.push(e);
        }
        out
    }

    pub fn achar(&self) -> Option<Visto> {
        self.vistos.first().copied()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Cada número de erro aparece uma vez só, como E e três dígitos, para
    /// quem relata um erro achar exatamente uma falha.
    #[test]
    fn numeros_de_erro_sao_unicos() {
        let fontes = [
            ("aparelho/mod.rs", include_str!("mod.rs")),
            ("aparelho/backup.rs", include_str!("backup.rs")),
            ("aparelho/bot.rs", include_str!("bot.rs")),
            ("aparelho/canal.rs", include_str!("canal.rs")),
            ("aparelho/dono.rs", include_str!("dono.rs")),
            ("aparelho/limpa.rs", include_str!("limpa.rs")),
            ("aparelho/real.rs", include_str!("real.rs")),
            ("aparelho/regras.rs", include_str!("regras.rs")),
            ("aparelho/sdcard.rs", include_str!("sdcard.rs")),
            ("aparelho/sim.rs", include_str!("sim.rs")),
            ("aparelho/spt.rs", include_str!("spt.rs")),
            ("aparelho/usb.rs", include_str!("usb.rs")),
            ("aparelho/yp3.rs", include_str!("yp3.rs")),
            ("pacote.rs", include_str!("../pacote.rs")),
            ("site.rs", include_str!("../site.rs")),
            ("storepod.rs", include_str!("../storepod.rs")),
            ("elevacao.rs", include_str!("../elevacao.rs")),
            ("trabalhador.rs", include_str!("../trabalhador.rs")),
        ];
        let mut vistos = std::collections::HashMap::new();
        for (arquivo, fonte) in fontes {
            let codigo = fonte.split("#[cfg(test)]").next().unwrap();
            for (i, pat) in codigo.match_indices(concat!("err", "!(")) {
                let linha = &codigo[..i].lines().last().unwrap_or("");
                if linha.trim_start().starts_with("//") {
                    continue;
                }
                let n = codigo[i + pat.len()..].split(',').next().unwrap().trim();
                let b = n.as_bytes();
                let ok = b.len() == 4 && b[0] == b'E' && (b'1'..=b'9').contains(&b[1]) && b[2..].iter().all(u8::is_ascii_digit);
                assert!(ok, "{arquivo}: {n:?} não é um número de erro como E214");
                if let Some(outro) = vistos.insert(n, arquivo) {
                    panic!("{n} aparece em {outro} e de novo em {arquivo}: use um ajudante só");
                }
            }
        }
        assert!(vistos.len() > 40, "só {} números de erro encontrados", vistos.len());
    }

    fn pacote_rockbox(dir: &std::path::Path) -> std::path::PathBuf {
        use crate::pacote::{hex, sha256};
        let img = vec![0x5au8; SETOR as usize];
        let cfg = b"theme: openpod128\nlang: /.rockbox/langs/portugues-brasileiro.lng\n";
        let manifest = b"formato=1\nlinha=rockbox\nversao=1.0\ndata=22/11/2026\ntitulo=OpenPod RockBox\n";
        let sums = format!("{}  flash/0x0000e000.bin\n{}  card/.rockbox/config.cfg\n", hex(&sha256(&img)), hex(&sha256(cfg)));
        let caminho = dir.join("rockbox.tar.gz");
        let f = std::fs::File::create(&caminho).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
        for (nome, dados) in [
            ("MANIFEST", manifest.as_slice()),
            ("SHA256SUMS", sums.as_bytes()),
            ("flash/0x0000e000.bin", img.as_slice()),
            ("card/.rockbox/config.cfg", cfg.as_slice()),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(dados.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, nome, dados).unwrap();
        }
        b.finish().unwrap();
        caminho
    }

    #[test]
    fn fluxo_completo_no_simulador() {
        let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("fluxo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pacote = pacote_rockbox(&dir);

        let mut hub = Hub::iniciar(Box::new(sim::Sim::new(2000.0)));
        hub.enviar(Comando::Assistir(true));
        let t0 = std::time::Instant::now();
        while hub.vistos.is_empty() && t0.elapsed() < Duration::from_secs(5) {
            hub.poll();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!hub.vistos.is_empty(), "o player simulado não apareceu");

        hub.enviar(Comando::Conferir);
        let mut pronto = false;
        while !pronto && t0.elapsed() < Duration::from_secs(5) {
            for e in hub.poll() {
                if matches!(e, Evento::Pronto) {
                    pronto = true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(pronto, "a conferência simulada não terminou");

        hub.enviar(Comando::Gravar { pacote, idioma: crate::textos::Idioma::Pt, lang_rockbox: None, limpar_rockbox: false });
        let mut feito = None;
        while feito.is_none() && t0.elapsed() < Duration::from_secs(10) {
            for e in hub.poll() {
                match e {
                    Evento::Feito(r) => feito = Some(r),
                    Evento::Falhou(e) => panic!("o fluxo falhou: {e}"),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let r = feito.expect("o fluxo simulado não terminou");
        assert!(r.backup.is_some());
        assert!(r.cartao);
        assert!(r.arquivos >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_hub_troca_de_aparelho() {
        let mut hub = Hub::iniciar(Box::new(sim::Sim::new(2000.0)));
        let rx = hub.trocar_backend(Box::new(sim::Sim::new(2000.0)));
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok(), "a troca não foi feita");
        hub.enviar(Comando::Assistir(true));
        let t0 = std::time::Instant::now();
        while hub.vistos.is_empty() && t0.elapsed() < Duration::from_secs(5) {
            hub.poll();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!hub.vistos.is_empty(), "o aparelho novo não apareceu");
    }

    struct ScanQuebrado;
    impl Backend for ScanQuebrado {
        fn scan(&mut self, _ev: &Sink) -> Vec<Visto> {
            panic!("scan quebrado")
        }
        fn run(&mut self, _cmd: Comando, ev: &Sink) -> Result<(), String> {
            ev.send(Evento::Pronto);
            Ok(())
        }
    }

    #[test]
    fn panico_no_scan_nao_mata_a_thread_do_aparelho() {
        let mut hub = Hub::iniciar(Box::new(ScanQuebrado));
        std::thread::sleep(Duration::from_millis(350));
        hub.enviar(Comando::Conferir);
        let t0 = std::time::Instant::now();
        let mut pronto = false;
        while !pronto && t0.elapsed() < Duration::from_secs(3) {
            for e in hub.poll() {
                if matches!(e, Evento::Pronto) {
                    pronto = true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(pronto, "a thread do aparelho morreu no pânico do scan");
    }
}
