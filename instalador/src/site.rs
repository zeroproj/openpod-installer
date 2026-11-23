//! O índice do site e o download do pacote escolhido.
//!
//! O índice fica em `https://zeroproj.github.io/OpenPod/pacotes/index.json`:
//! `{"system":{"versao","data","url","sha256"},"rockbox":{...}}`. Enquanto
//! ele não estiver publicado, o instalador diz "sem conexão com o site" e
//! oferece "Escolher arquivo…".

use crate::err;
use crate::pacote::{hex, sha256, unhex32, Tipo};
use std::path::PathBuf;
use std::time::Duration;

pub const URL_INDICE: &str = "https://zeroproj.github.io/OpenPod/pacotes/index.json";

fn sem_site(e: ureq::Error) -> String {
    err!(E601, "não consegui falar com o site: {e}")
}
fn leitura_falhou(e: ureq::Error) -> String {
    err!(E602, "não consegui ler o que o site mandou: {e}")
}
fn resposta(status: u16) -> String {
    err!(E603, "o site respondeu {status}")
}

#[derive(Clone, Debug)]
pub struct Versao {
    pub versao: String,
    pub data: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default)]
pub struct Indice {
    pub system: Option<Versao>,
    pub rockbox: Option<Versao>,
}

impl Indice {
    pub fn de(&self, tipo: Tipo) -> Option<&Versao> {
        match tipo {
            Tipo::System => self.system.as_ref(),
            Tipo::Rockbox => self.rockbox.as_ref(),
        }
    }
}

/// O agente que fala com o site (o mesmo para o índice e para a StorePod).
pub(crate) fn agente() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("openpod-tune/0.1")
        .https_only(true)
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn versao(v: &serde_json::Value) -> Option<Versao> {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Some(Versao { versao: s("versao")?, data: s("data").unwrap_or_default(), url: s("url")?, sha256: s("sha256")? })
}

pub fn buscar() -> Result<Indice, String> {
    let mut res = agente().get(URL_INDICE).call().map_err(sem_site)?;
    let status = res.status().as_u16();
    let corpo = res.body_mut().with_config().limit(1 << 20).read_to_string().map_err(leitura_falhou)?;
    if status != 200 {
        return Err(resposta(status));
    }
    let v: serde_json::Value = serde_json::from_str(&corpo).map_err(|e| err!(E604, "o índice do site não faz sentido: {e}"))?;
    Ok(Indice { system: v.get("system").and_then(versao), rockbox: v.get("rockbox").and_then(versao) })
}

pub fn pasta_pacotes() -> PathBuf {
    super::aparelho::backup::pasta().join("pacotes")
}

/// Baixa o pacote do site e confere o SHA-256 do índice.
pub fn baixar(v: &Versao) -> Result<PathBuf, String> {
    let quer = unhex32(&v.sha256).ok_or_else(|| err!(E605, "o índice do site tem um sha256 ruim"))?;
    let nome = v.url.rsplit('/').next().filter(|n| n.ends_with(".tar.gz")).unwrap_or("openpod-pacote.tar.gz");
    let dir = pasta_pacotes();
    std::fs::create_dir_all(&dir).map_err(|e| err!(E606, "pasta de pacotes {}: {e}", dir.display()))?;
    let destino = dir.join(nome);
    let mut res = agente().get(&v.url).call().map_err(sem_site)?;
    let status = res.status().as_u16();
    if status != 200 {
        return Err(resposta(status));
    }
    let dados = res.body_mut().with_config().limit(128 << 20).read_to_vec().map_err(leitura_falhou)?;
    if sha256(&dados) != quer {
        return Err(err!(E607, "o pacote baixado está corrompido (sha256 {}…, esperado {}…)", &hex(&sha256(&dados))[..8], &v.sha256[..8]));
    }
    let tmp = dir.join(format!(".{nome}.part"));
    std::fs::write(&tmp, &dados).map_err(|e| err!(E608, "gravar o pacote: {e}"))?;
    std::fs::rename(&tmp, &destino).map_err(|e| err!(E609, "guardar o pacote: {e}"))?;
    Ok(destino)
}
