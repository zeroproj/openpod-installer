//! O canal entre a janela e o trabalhador elevado: um socket Unix dentro de
//! uma pasta `0700`, mensagens em linhas JSON (os mesmos `Comando` e
//! `Evento` do aparelho) e um token de 32 bytes conferido na conexão.
//!
//! Quem escuta é a janela; quem se liga é o trabalhador (que roda elevado).

use crate::aparelho::{Backend, Comando, Evento, Sink, Visto};
use crate::err;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};

/// Uma mensagem do protocolo: uma linha JSON por mensagem.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Linha {
    /// a primeira mensagem do trabalhador: o token
    Token { token: String },
    /// a janela aceitou o token
    Aceito,
    /// a janela recusou o token
    Recusado { motivo: String },
    /// a janela mandou um comando para o aparelho
    Comando { cmd: Comando },
    /// o trabalhador respondeu com um evento
    Evento { ev: Evento },
}

/// 32 bytes de não-determinismo, em hexadecimal.
pub fn token() -> Result<String, String> {
    let mut b = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| err!(E706, "não consegui gerar o token: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Compara dois tokens sem deixar o tempo vazar o segredo.
pub fn mesmos(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for i in 0..a.len() {
        d |= a[i] ^ b[i];
    }
    d == 0
}

/// A ponta que lê as linhas.
pub struct Leitor {
    fluxo: UnixStream,
    buf: Vec<u8>,
    pos: usize,
}

/// A ponta que escreve as linhas.
pub struct Escritor {
    fluxo: UnixStream,
}

/// O tempo de espera de uma leitura do canal.
const ESPERA: Duration = Duration::from_secs(30);

/// O canal pronto: as duas pontas do mesmo socket.
pub struct Conexao {
    leitor: Leitor,
    escritor: Escritor,
}

impl Conexao {
    pub fn nova(fluxo: UnixStream) -> Result<Conexao, String> {
        let copia = fluxo.try_clone().map_err(|e| err!(E702, "clonar o socket: {e}"))?;
        // no macOS o socket aceito herda o modo não-bloqueante do
        // listener: a leitura tem que esperar de verdade
        let _ = fluxo.set_nonblocking(false);
        let _ = fluxo.set_read_timeout(Some(ESPERA));
        Ok(Conexao { leitor: Leitor { fluxo, buf: Vec::new(), pos: 0 }, escritor: Escritor { fluxo: copia } })
    }

    /// Separa as pontas: o trabalhador lê numa thread e escreve em outra.
    pub fn dividir(self) -> (Leitor, Escritor) {
        (self.leitor, self.escritor)
    }

    pub fn enviar(&mut self, l: &Linha) -> Result<(), String> {
        self.escritor.enviar(l)
    }

    pub fn receber(&mut self) -> Result<Linha, String> {
        self.leitor.receber()
    }
}

impl Leitor {
    /// A próxima linha, já como mensagem do protocolo.
    pub fn receber(&mut self) -> Result<Linha, String> {
        let linha = self.texto()?;
        serde_json::from_str(&linha).map_err(|e| err!(E711, "linha do canal inválida: {e}"))
    }

    /// A próxima linha, como texto.
    pub fn texto(&mut self) -> Result<String, String> {
        loop {
            if let Some(i) = self.buf[self.pos..].iter().position(|&b| b == b'\n') {
                let ini = self.pos;
                let fim = ini + i;
                self.pos = fim + 1;
                return Ok(String::from_utf8_lossy(&self.buf[ini..fim]).trim().to_string());
            }
            if self.pos > 0 {
                self.buf.drain(..self.pos);
                self.pos = 0;
            }
            let mut tmp = [0u8; 4096];
            match self.fluxo.read(&mut tmp) {
                Ok(0) => return Err(err!(E710, "o outro lado do canal fechou")),
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(err!(E722, "o outro lado do canal não respondeu a tempo"))
                }
                Err(e) => return Err(err!(E709, "ler do canal: {e}")),
            }
        }
    }
}

impl Escritor {
    pub fn enviar(&mut self, l: &Linha) -> Result<(), String> {
        let mut s = serde_json::to_string(l).map_err(|e| err!(E712, "serializar a mensagem: {e}"))?;
        s.push('\n');
        self.fluxo.write_all(s.as_bytes()).map_err(|e| err!(E713, "escrever no canal: {e}"))
    }
}

/// O trabalhador se liga no socket e diz o token.
pub fn conectar(caminho: &Path, token: &str, espera: Duration) -> Result<Conexao, String> {
    let t0 = Instant::now();
    loop {
        match UnixStream::connect(caminho) {
            Ok(fluxo) => {
                let mut con = Conexao::nova(fluxo)?;
                con.enviar(&Linha::Token { token: token.to_string() })?;
                return match con.receber()? {
                    Linha::Aceito => Ok(con),
                    Linha::Recusado { motivo } => Err(err!(E715, "o token foi recusado: {motivo}")),
                    outra => Err(err!(E717, "resposta inesperada do canal: {outra:?}")),
                };
            }
            Err(e) => {
                if t0.elapsed() >= espera {
                    return Err(err!(E714, "não consegui ligar no canal do trabalhador: {e}"));
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    }
}

/// A janela espera o trabalhador e confere o token.
pub fn aceitar(listener: &UnixListener, token: &str, espera: Duration) -> Result<Conexao, String> {
    aceitar_com(listener, token, espera, &|| false)
}

/// Igual a `aceitar`, mas desiste quando `aborta` responder sim (o macOS
/// recusou a senha, por exemplo).
pub fn aceitar_com(listener: &UnixListener, token: &str, espera: Duration, aborta: &dyn Fn() -> bool) -> Result<Conexao, String> {
    listener.set_nonblocking(true).map_err(|e| err!(E716, "o socket não entrou no modo de espera: {e}"))?;
    let t0 = Instant::now();
    loop {
        match listener.accept() {
            Ok((fluxo, _)) => {
                let mut con = Conexao::nova(fluxo)?;
                return match con.receber()? {
                    Linha::Token { token: veio } if mesmos(&veio, token) => {
                        con.enviar(&Linha::Aceito)?;
                        Ok(con)
                    }
                    Linha::Token { .. } => {
                        let _ = con.enviar(&Linha::Recusado { motivo: "token errado".into() });
                        Err(err!(E718, "o token do trabalhador não bate"))
                    }
                    outra => {
                        let _ = con.enviar(&Linha::Recusado { motivo: "primeira mensagem errada".into() });
                        Err(err!(E733, "resposta inesperada do canal: {outra:?}"))
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if aborta() {
                    return Err(err!(E719, "o macOS recusou o pedido de senha"));
                }
                if t0.elapsed() >= espera {
                    return Err(err!(E731, "o trabalhador não apareceu a tempo"));
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(err!(E720, "esperar o trabalhador: {e}")),
        }
    }
}

/// O aparelho do outro lado do canal: a janela fala com o trabalhador
/// elevado por aqui.
pub struct Canal {
    con: Conexao,
}

impl Canal {
    pub fn nova(con: Conexao) -> Canal {
        Canal { con }
    }

    fn enviar(&mut self, cmd: &Comando) -> Result<(), String> {
        self.con.enviar(&Linha::Comando { cmd: cmd.clone() })
    }

    fn evento(&mut self) -> Result<Evento, String> {
        match self.con.receber()? {
            Linha::Evento { ev } => Ok(ev),
            outra => Err(err!(E734, "resposta inesperada do canal: {outra:?}")),
        }
    }
}

impl Backend for Canal {
    fn scan(&mut self, _ev: &Sink) -> Vec<Visto> {
        if self.enviar(&Comando::Procurar).is_err() {
            return Vec::new();
        }
        match self.evento() {
            Ok(Evento::Presenca(v)) => v,
            _ => Vec::new(),
        }
    }

    fn run(&mut self, cmd: Comando, ev: &Sink) -> Result<(), String> {
        self.enviar(&cmd)?;
        loop {
            let e = self.evento()?;
            let fim = matches!(e, Evento::Pronto | Evento::Feito(_) | Evento::Falhou(_));
            ev.send(e);
            if fim {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn caminho(nome: &str) -> std::path::PathBuf {
        let dir = std::env::current_dir().unwrap().join(".tmp");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("canal-{}-{}.sock", nome, std::process::id()))
    }

    #[test]
    fn o_protocolo_json_volta_igual() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut escrita = Conexao::nova(a).unwrap();
        let mut leitura = Conexao::nova(b).unwrap();

        let cmd = Comando::Gravar {
            pacote: std::path::PathBuf::from("/Users/um/Pacote/openpod.tar.gz"),
            idioma: crate::textos::Idioma::En,
            lang_rockbox: Some("english.lng".to_string()),
            limpar_rockbox: true,
        };
        escrita.enviar(&Linha::Comando { cmd: cmd.clone() }).unwrap();
        match leitura.receber().unwrap() {
            Linha::Comando { cmd: veio } => {
                assert_eq!(veio, cmd, "o comando voltou diferente");
            }
            outra => panic!("veio outra coisa: {outra:?}"),
        }

        let ev = Evento::Bateria(4040);
        escrita.enviar(&Linha::Evento { ev: ev.clone() }).unwrap();
        match leitura.receber().unwrap() {
            Linha::Evento { ev: veio } => assert_eq!(veio, ev, "o evento voltou diferente"),
            outra => panic!("veio outra coisa: {outra:?}"),
        }

        let copia = Evento::Copia(std::path::PathBuf::from("/Users/um/Documents/OpenPod/backups/x.bin"));
        escrita.enviar(&Linha::Evento { ev: copia.clone() }).unwrap();
        match leitura.receber().unwrap() {
            Linha::Evento { ev: veio } => assert_eq!(veio, copia),
            outra => panic!("veio outra coisa: {outra:?}"),
        }
    }

    /// A linha do comando sem os campos novos (a janela de outra versão)
    /// ainda é lida: o canal continua compatível.
    #[test]
    fn a_linha_do_comando_velha_tem_lida() {
        let linha = r#"{"k":"comando","cmd":{"Gravar":{"pacote":"/Users/um/x.tar.gz","idioma":"En"}}}"#;
        match serde_json::from_str::<Linha>(linha).unwrap() {
            Linha::Comando { cmd: Comando::Gravar { idioma, lang_rockbox, limpar_rockbox, .. } } => {
                assert_eq!(idioma, crate::textos::Idioma::En);
                assert!(lang_rockbox.is_none(), "sem o campo: o idioma é o do instalador");
                assert!(!limpar_rockbox, "sem o campo: não começa do zero");
            }
            outra => panic!("veio outra coisa: {outra:?}"),
        }
    }

    #[test]
    fn a_janela_fala_com_o_trabalhador() {
        use crate::aparelho::Modo;
        use std::sync::mpsc::channel;

        let caminho = caminho("fala");
        let _ = std::fs::remove_file(&caminho);
        let listener = UnixListener::bind(&caminho).unwrap();
        let token = token().unwrap();
        let do_servidor = token.clone();

        // um trabalhador de mentira: responde ao Procurar e ao Conferir
        let servidor = std::thread::spawn(move || {
            let mut con = aceitar(&listener, &do_servidor, Duration::from_secs(5)).unwrap();
            loop {
                let linha = match con.receber() {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let cmd = match linha {
                    Linha::Comando { cmd } => cmd,
                    _ => continue,
                };
                let ev = match cmd {
                    Comando::Procurar => Evento::Presenca(vec![Visto { modo: Modo::Rom, acesso: true }]),
                    Comando::Conferir => Evento::Pronto,
                    _ => continue,
                };
                if con.enviar(&Linha::Evento { ev }).is_err() {
                    break;
                }
            }
        });

        let mut canal = Canal::nova(conectar(&caminho, &token, Duration::from_secs(5)).unwrap());
        let (tx, rx) = channel();
        let sink = crate::aparelho::Sink(tx);

        let vistos = canal.scan(&sink);
        assert_eq!(vistos.len(), 1, "o trabalhador não disse o que viu");
        assert_eq!(vistos[0].modo, Modo::Rom);

        canal.run(Comando::Conferir, &sink).unwrap();
        let eventos: Vec<Evento> = rx.try_iter().collect();
        assert!(eventos.iter().any(|e| *e == Evento::Pronto), "o evento não chegou: {eventos:?}");
        assert!(!eventos.iter().any(|e| matches!(e, Evento::Falhou(_))), "falhou: {eventos:?}");

        drop(canal);
        servidor.join().unwrap();
        let _ = std::fs::remove_file(&caminho);
    }

    #[test]
    fn o_token_errado_e_recusado() {
        let caminho = caminho("token");
        let _ = std::fs::remove_file(&caminho);
        let listener = UnixListener::bind(&caminho).unwrap();
        let certo = "a".repeat(64);

        let servidor = std::thread::spawn(move || aceitar(&listener, &certo, Duration::from_secs(5)));
        let cliente = conectar(&caminho, &"b".repeat(64), Duration::from_secs(5));
        let r = servidor.join().unwrap();
        assert!(r.is_err(), "o servidor não pode aceitar o token errado");
        assert!(cliente.is_err(), "o cliente tem que ver a recusa");
        let _ = std::fs::remove_file(&caminho);
    }

    #[test]
    fn o_token_certo_passa_e_o_trabalhador_fala() {
        let caminho = caminho("certo");
        let _ = std::fs::remove_file(&caminho);
        let listener = UnixListener::bind(&caminho).unwrap();
        let token = token().unwrap();
        let do_servidor = token.clone();

        let servidor = std::thread::spawn(move || {
            let mut con = aceitar(&listener, &do_servidor, Duration::from_secs(5))?;
            match con.receber()? {
                Linha::Comando { cmd: _ } => con.enviar(&Linha::Evento { ev: Evento::Bateria(3900) })?,
                _ => {}
            }
            match con.receber()? {
                Linha::Comando { cmd: _ } => con.enviar(&Linha::Evento { ev: Evento::Pronto })?,
                _ => {}
            }
            Ok::<(), String>(())
        });

        let mut con = conectar(&caminho, &token, Duration::from_secs(5)).unwrap();
        con.enviar(&Linha::Comando { cmd: Comando::Conferir }).unwrap();
        match con.receber().unwrap() {
            Linha::Evento { ev: Evento::Bateria(mv) } => assert_eq!(mv, 3900),
            outra => panic!("veio outra coisa: {outra:?}"),
        }
        con.enviar(&Linha::Comando { cmd: Comando::Conferir }).unwrap();
        match con.receber().unwrap() {
            Linha::Evento { ev: Evento::Pronto } => {}
            outra => panic!("veio outra coisa: {outra:?}"),
        }
        servidor.join().unwrap().unwrap();
        let _ = std::fs::remove_file(&caminho);
    }
}
