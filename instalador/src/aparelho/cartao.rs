//! O plano do cartão: o que copiar e como o tema entra no `config.cfg`.
//!
//! O pacote não leva `.rockbox/config.cfg`. Ele leva o modelo
//! `.rockbox/config-openpod.cfg` com as diretivas reais do OpenPod128 (as
//! mesmas de `.rockbox/themes/OpenPod128.cfg`). O Rockbox não tem a opção
//! `theme:`; ela é ignorada ao carregar o config. Por isso o instalador:
//! - sem `config.cfg`: cria um a partir do modelo, com o idioma escolhido;
//! - com `config.cfg`: troca só as chaves do tema e o idioma, preservando
//!   volume, EQ e as demais preferências do usuário.

use crate::textos::Idioma;

pub const MODELO: &str = ".rockbox/config-openpod.cfg";
pub const CONFIG: &str = ".rockbox/config.cfg";
pub const TEMA: &str = "OpenPod128";
pub const CFG_TEMA: &str = ".rockbox/themes/OpenPod128.cfg";
/// Onde os idiomas do RockBox ficam no cartão.
pub const PASTA_LANG: &str = ".rockbox/langs/";

/// O modelo de reserva, caso o pacote não traga nem o modelo nem o cfg do tema.
/// São as diretivas reais do OpenPod128.cfg, sem a chave inexistente `theme:`.
pub const MODELO_PADRAO: &str = "\
wps: /.rockbox/wps/OpenPod128.wps
sbs: /.rockbox/wps/OpenPod128.sbs
font: /.rockbox/fonts/12-Inter-Medium.fnt
iconset: /.rockbox/icons/OpenPod128.bmp
viewers iconset: -
statusbar: off
scrollbar: off
show icons: on
foreground color: FFFFFF
background color: 000000
selector type: bar (color)
line selector start color: 000000
line selector text color: FFFFFF
filetype colours: -
backdrop: -
ui viewport: -
";

#[derive(Clone, Debug)]
pub struct Plano {
    /// os arquivos que vão para o cartão, na ordem
    pub copiar: Vec<(String, Vec<u8>)>,
    /// um `config.cfg` novo foi criado a partir do modelo
    pub config_novo: bool,
    /// o cartão já tinha `config.cfg`: foi preservado, só o tema e o idioma mudaram
    pub config_mantida: bool,
}

/// O arquivo de idioma do RockBox do instalador: o padrão, quando o
/// usuário não escolhe outro na lista.
pub fn arquivo_do_idioma(idioma: Idioma) -> &'static str {
    match idioma {
        Idioma::Pt => "portugues-brasileiro.lng",
        Idioma::En => "english.lng",
    }
}

/// O caminho do arquivo de idioma no cartão.
pub fn caminho_do_lang(arquivo: &str) -> String {
    format!("/{PASTA_LANG}{arquivo}")
}

/// O caminho do idioma no cartão: o escolhido, ou o padrão do instalador.
pub fn idioma_do_rockbox(idioma: Idioma, escolhido: Option<&str>) -> String {
    match escolhido {
        Some(arquivo) => caminho_do_lang(arquivo),
        None => caminho_do_lang(arquivo_do_idioma(idioma)),
    }
}

/// O idioma escolhido só vale se o arquivo estiver no pacote: um nome de
/// fora (ou um caminho) não vai para o config.
fn lang_do_pacote<'a>(arquivos: &[(String, Vec<u8>)], escolhido: Option<&'a str>) -> Option<&'a str> {
    let nome = escolhido?;
    if !crate::pacote::nome_de_lang_ok(nome) {
        return None;
    }
    let caminho = format!("{PASTA_LANG}{nome}");
    arquivos.iter().any(|(n, _)| n == &caminho).then_some(nome)
}

/// A chave de uma linha `chave: valor`; `None` para comentário ou linha vazia.
fn chave(linha: &str) -> Option<&str> {
    let l = linha.trim();
    if l.is_empty() || l.starts_with('#') {
        return None;
    }
    l.split_once(':').map(|(k, _)| k.trim())
}

fn definicoes(texto: &str) -> Vec<String> {
    texto.lines().filter(|l| chave(l).is_some()).map(|l| l.trim().to_string()).collect()
}

/// Só as chaves do tema (sem `theme:` e sem `lang:`, que é do instalador).
fn linhas_do_tema(texto: &str) -> Vec<String> {
    definicoes(texto).into_iter().filter(|l| !matches!(chave(l), Some("theme") | Some("lang"))).collect()
}

/// O texto de onde saem as diretivas do tema. Pacotes antigos traziam só
/// `theme: OpenPod128`; nesse caso usamos o `.rockbox/themes/OpenPod128.cfg`
/// que veio no próprio pacote.
fn texto_do_tema(modelo: Option<&[u8]>, arquivos: &[(String, Vec<u8>)]) -> String {
    let modelo = modelo.map(|m| String::from_utf8_lossy(m).into_owned());
    if modelo.as_deref().map(|t| !linhas_do_tema(t).is_empty()).unwrap_or(false) {
        return modelo.unwrap_or_default();
    }
    let nome = modelo
        .as_deref()
        .and_then(|t| {
            definicoes(t).into_iter().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k.trim() == "theme").then(|| v.trim().to_string())
            })
        })
        .unwrap_or_else(|| TEMA.to_string());
    let caminho = format!(".rockbox/themes/{nome}.cfg");
    if let Some((_, dados)) = arquivos.iter().find(|(n, _)| n == &caminho) {
        return String::from_utf8_lossy(dados).into_owned();
    }
    MODELO_PADRAO.to_string()
}

fn config_do_modelo(modelo: &str, idioma: Idioma, lang: Option<&str>) -> Vec<u8> {
    let mut linhas = linhas_do_tema(modelo);
    if linhas.is_empty() {
        linhas = linhas_do_tema(MODELO_PADRAO);
    }
    linhas.push(format!("lang: {}", idioma_do_rockbox(idioma, lang)));
    let mut out = linhas.join("\n");
    out.push('\n');
    out.into_bytes()
}

/// Aplica o tema a um config existente sem apagar o que é do usuário: remove
/// as chaves que pertencem ao tema e o idioma, mantém o resto e acrescenta as
/// diretivas do tema e o idioma escolhido.
fn config_com_tema(existente: &[u8], modelo: &str, idioma: Idioma, lang: Option<&str>) -> Vec<u8> {
    let mut do_tema = linhas_do_tema(modelo);
    if do_tema.is_empty() {
        do_tema = linhas_do_tema(MODELO_PADRAO);
    }
    let chaves: std::collections::HashSet<String> = do_tema.iter().filter_map(|l| chave(l).map(str::to_string)).collect();
    let texto = String::from_utf8_lossy(existente);
    let mut linhas: Vec<String> = texto
        .lines()
        .filter(|l| match chave(l) {
            Some(k) => k != "lang" && k != "theme" && !chaves.contains(k),
            None => true,
        })
        .map(|l| l.trim_end().to_string())
        .collect();
    while linhas.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        linhas.pop();
    }
    linhas.extend(do_tema);
    linhas.push(format!("lang: {}", idioma_do_rockbox(idioma, lang)));
    let mut out = linhas.join("\n");
    out.push('\n');
    out.into_bytes()
}

/// Decide o que vai para o cartão. `arquivos` é o `card/` do pacote;
/// `existente` é o `config.cfg` que já estava no cartão, se houver.
/// `lang` é o arquivo de idioma escolhido pelo usuário (o nome do `.lng`):
/// só vale se o pacote trouxer esse arquivo.
pub fn planejar(arquivos: Vec<(String, Vec<u8>)>, existente: Option<Vec<u8>>, idioma: Idioma, lang: Option<&str>) -> Plano {
    let lang = lang_do_pacote(&arquivos, lang);
    let modelo = arquivos.iter().find(|(n, _)| n == MODELO).map(|(_, d)| d.clone());
    let texto = texto_do_tema(modelo.as_deref(), &arquivos);
    let mut copiar: Vec<(String, Vec<u8>)> = arquivos.into_iter().filter(|(n, _)| n != MODELO && n != CONFIG).collect();
    match existente {
        Some(antigo) => {
            copiar.push((CONFIG.to_string(), config_com_tema(&antigo, &texto, idioma, lang)));
            Plano { copiar, config_novo: false, config_mantida: true }
        }
        None => {
            copiar.push((CONFIG.to_string(), config_do_modelo(&texto, idioma, lang)));
            Plano { copiar, config_novo: true, config_mantida: false }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRETIVAS: &str = "\
wps: /.rockbox/wps/OpenPod128.wps
sbs: /.rockbox/wps/OpenPod128.sbs
font: /.rockbox/fonts/12-Inter-Medium.fnt
iconset: /.rockbox/icons/OpenPod128.bmp
viewers iconset: -
statusbar: off
scrollbar: off
show icons: on
foreground color: FFFFFF
background color: 000000
selector type: bar (color)
line selector start color: 000000
line selector text color: FFFFFF
filetype colours: -
backdrop: -
ui viewport: -
";

    fn modelo_novo() -> (String, Vec<u8>) {
        (MODELO.to_string(), format!("{DIRETIVAS}lang: /.rockbox/langs/portugues-brasileiro.lng\n").into_bytes())
    }

    fn modelo_antigo() -> (String, Vec<u8>) {
        (MODELO.to_string(), b"theme: OpenPod128\nlang: /.rockbox/langs/portugues-brasileiro.lng\n".to_vec())
    }

    fn tema_cfg() -> (String, Vec<u8>) {
        (CFG_TEMA.to_string(), format!("# OpenPod128\n{DIRETIVAS}").into_bytes())
    }

    /// Um arquivo de idioma do pacote (o `card/.rockbox/langs/`).
    fn lang(caminho: &str) -> (String, Vec<u8>) {
        (format!("{PASTA_LANG}{caminho}"), b"idioma".to_vec())
    }

    fn cfg(p: &Plano) -> String {
        let (_, dados) = p.copiar.iter().find(|(n, _)| n == CONFIG).expect("config.cfg no plano");
        String::from_utf8_lossy(dados).into_owned()
    }

    fn confere_diretivas(texto: &str) {
        for chave in [
            "wps: /.rockbox/wps/OpenPod128.wps",
            "sbs: /.rockbox/wps/OpenPod128.sbs",
            "font: /.rockbox/fonts/12-Inter-Medium.fnt",
            "iconset: /.rockbox/icons/OpenPod128.bmp",
            "viewers iconset: -",
            "statusbar: off",
            "scrollbar: off",
            "show icons: on",
            "foreground color: FFFFFF",
            "background color: 000000",
            "selector type: bar (color)",
            "line selector start color: 000000",
            "line selector text color: FFFFFF",
            "filetype colours: -",
            "backdrop: -",
            "ui viewport: -",
        ] {
            assert!(texto.lines().any(|l| l.trim() == chave), "faltou {chave:?} em:\n{texto}");
        }
        assert!(!texto.contains("theme:"), "o Rockbox não tem a opção theme: ->\n{texto}");
    }

    #[test]
    fn sem_config_cria_do_modelo_no_idioma_do_instalador() {
        let arquivos = vec![
            tema_cfg(),
            modelo_novo(),
            (CONFIG.to_string(), b"config antigo do pacote\n".to_vec()),
            (".rockbox/rocks/x.rock".into(), b"x".to_vec()),
        ];
        let p = planejar(arquivos, None, Idioma::En, None);
        assert!(p.config_novo);
        assert!(!p.config_mantida);
        assert!(!p.copiar.iter().any(|(n, _)| n == MODELO), "o modelo não vai para o cartão");
        assert!(!p.copiar.iter().any(|(n, d)| n == CONFIG && d == b"config antigo do pacote\n"));
        let texto = cfg(&p);
        confere_diretivas(&texto);
        assert!(texto.contains("lang: /.rockbox/langs/english.lng"));
        assert!(!texto.contains("portugues-brasileiro"));
        assert!(p.copiar.iter().any(|(n, _)| n == ".rockbox/rocks/x.rock"));
        assert!(p.copiar.iter().any(|(n, _)| n == CFG_TEMA), "o cfg do tema continua no cartão");

        let p = planejar(vec![modelo_novo()], None, Idioma::Pt, None);
        assert!(cfg(&p).contains("lang: /.rockbox/langs/portugues-brasileiro.lng"));
    }

    #[test]
    fn com_config_aplica_so_o_tema_e_preserva_as_preferencias() {
        let antigo = b"volume: -10\nequalizer enabled: yes\nwps: /.rockbox/wps/Outro.wps\nfont: /.rockbox/fonts/outra.fnt\nshow icons: off\nlang: /.rockbox/langs/english.lng\n# minhas notas\n";
        let p = planejar(vec![modelo_novo()], Some(antigo.to_vec()), Idioma::Pt, None);
        assert!(!p.config_novo);
        assert!(p.config_mantida);
        let texto = cfg(&p);
        assert!(texto.contains("volume: -10"), "o volume do usuário fica:\n{texto}");
        assert!(texto.contains("equalizer enabled: yes"), "o EQ do usuário fica:\n{texto}");
        assert!(texto.contains("# minhas notas"), "comentários do usuário ficam");
        assert!(!texto.contains("Outro.wps"), "o wps antigo é do tema e sai");
        assert!(!texto.contains("outra.fnt"), "a fonte antiga é do tema e sai");
        confere_diretivas(&texto);
        assert!(texto.contains("lang: /.rockbox/langs/portugues-brasileiro.lng"));
        assert_eq!(texto.matches("lang:").count(), 1, "um só idioma:\n{texto}");
        assert_eq!(texto.matches("wps:").count(), 1, "um só wps:\n{texto}");
        assert!(!texto.contains("english.lng"));
    }

    #[test]
    fn modelo_antigo_resolve_as_diretivas_do_cfg_do_tema() {
        // o pacote 822b9f5e99 trazia só "theme: OpenPod128"; o cfg do tema
        // veio no mesmo pacote e é dele que saem as diretivas.
        let p = planejar(vec![modelo_antigo(), tema_cfg()], None, Idioma::Pt, None);
        let texto = cfg(&p);
        confere_diretivas(&texto);
        assert!(texto.contains("lang: /.rockbox/langs/portugues-brasileiro.lng"));

        // com config existente também: tema aplicado, volume mantido.
        let p = planejar(vec![modelo_antigo(), tema_cfg()], Some(b"volume: -20\ntheme: MeuTema\n".to_vec()), Idioma::En, None);
        let texto = cfg(&p);
        assert!(texto.contains("volume: -20"));
        assert!(texto.contains("lang: /.rockbox/langs/english.lng"));
        confere_diretivas(&texto);
    }

    #[test]
    fn sem_modelo_usa_o_padrao() {
        let p = planejar(vec![], None, Idioma::Pt, None);
        let texto = cfg(&p);
        confere_diretivas(&texto);
        assert!(texto.ends_with("lang: /.rockbox/langs/portugues-brasileiro.lng\n"));
    }

    #[test]
    fn reler_e_replanejar_nao_muda_nem_duplica() {
        let primeira = planejar(vec![modelo_novo()], None, Idioma::Pt, None);
        let um = cfg(&primeira);
        let segunda = planejar(vec![modelo_novo()], Some(um.clone().into_bytes()), Idioma::Pt, None);
        let dois = cfg(&segunda);
        assert_eq!(um, dois, "replanejar sobre o próprio resultado tem de ser estável");
        assert!(segunda.config_mantida);

        let terceira = planejar(vec![modelo_novo()], Some(dois.into_bytes()), Idioma::En, None);
        let tres = cfg(&terceira);
        assert!(tres.contains("lang: /.rockbox/langs/english.lng"));
        assert!(!tres.contains("portugues-brasileiro"));
        assert_eq!(tres.matches("wps:").count(), 1);
    }

    /// O idioma escolhido pelo usuário é o que vai para o `config.cfg`, com
    /// config novo e com config existente.
    #[test]
    fn o_idioma_escolhido_vai_para_o_config() {
        let arquivos = vec![modelo_novo(), lang("english.lng"), lang("portugues-brasileiro.lng"), lang("deutsch.lng")];
        // config novo
        let p = planejar(arquivos.clone(), None, Idioma::Pt, Some("deutsch.lng"));
        let texto = cfg(&p);
        assert!(texto.contains("lang: /.rockbox/langs/deutsch.lng"), "{texto}");
        assert_eq!(texto.matches("lang:").count(), 1, "um só idioma:\n{texto}");
        assert!(!texto.contains("portugues-brasileiro"), "{texto}");
        // config existente: o tema entra, as preferências do usuário ficam
        let antigo = b"volume: -10\nlang: /.rockbox/langs/english.lng\n";
        let p = planejar(arquivos, Some(antigo.to_vec()), Idioma::Pt, Some("deutsch.lng"));
        let texto = cfg(&p);
        assert!(texto.contains("lang: /.rockbox/langs/deutsch.lng"), "{texto}");
        assert!(texto.contains("volume: -10"), "{texto}");
        assert!(!texto.contains("english.lng"), "{texto}");
        assert_eq!(texto.matches("lang:").count(), 1, "um só idioma:\n{texto}");
        // sem escolha: o padrão do instalador
        let p = planejar(vec![modelo_novo()], None, Idioma::Pt, None);
        assert!(cfg(&p).contains("lang: /.rockbox/langs/portugues-brasileiro.lng"));
    }

    /// Um idioma que o pacote não traz (ou um caminho malicioso) não vai
    /// para o config: o padrão fica.
    #[test]
    fn o_idioma_fora_do_pacote_e_ignorado() {
        let arquivos = vec![modelo_novo(), lang("english.lng")];
        // um nome que o pacote não traz
        let p = planejar(arquivos.clone(), None, Idioma::Pt, Some("francais.lng"));
        let texto = cfg(&p);
        assert!(texto.contains("lang: /.rockbox/langs/portugues-brasileiro.lng"), "{texto}");
        assert!(!texto.contains("francais"), "{texto}");
        // e um caminho malicioso
        for nome in ["../etc/passwd.lng", "/etc/passwd.lng", ".rockbox/langs/english.lng", "english.lng/../x.lng", "..lng"] {
            let p = planejar(arquivos.clone(), None, Idioma::Pt, Some(nome));
            let texto = cfg(&p);
            assert!(!texto.contains("passwd"), "{nome}: {texto}");
            assert!(!texto.contains(".."), "{nome}: {texto}");
            assert!(texto.contains("lang: /.rockbox/langs/portugues-brasileiro.lng"), "{nome}: {texto}");
        }
    }
}
