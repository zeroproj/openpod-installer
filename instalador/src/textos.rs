//! Português e inglês, numa tabela única, seguindo o idioma do sistema.
//! Tudo em linguagem simples, sem jargão.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Idioma {
    Pt,
    En,
}

impl Idioma {
    pub fn detectar() -> Idioma {
        let loc = sys_locale::get_locale().unwrap_or_default().to_ascii_lowercase();
        if loc.starts_with("pt") {
            Idioma::Pt
        } else {
            Idioma::En
        }
    }
}

/// Os idiomas do RockBox, pelo nome do arquivo (sem o `.lng`) e pelo nome
/// que aparece na lista: o do próprio idioma (endônimo).
///
/// Os idiomas cujo alfabeto a fonte da janela não tem (chinês, japonês,
/// coreano, árabe, hebraico, híndi, tailandês) ficam com o nome em
/// latim: o endônimo sairia como quadradinho vazio na tela.
pub const IDIOMAS: &[(&str, &str)] = &[
    ("portugues-brasileiro", "Português (Brasil)"),
    ("portugues", "Português (Portugal)"),
    ("english", "English (UK)"),
    ("english-us", "English (US)"),
    ("espanol", "Español"),
    ("deutsch", "Deutsch"),
    ("francais", "Français"),
    ("italiano", "Italiano"),
    ("arabic", "Arabic"),
    ("basque", "Euskara"),
    ("bulgarian", "Български"),
    ("catala", "Català"),
    ("chinese-simp", "Chinese (Simplified)"),
    ("chinese-trad", "Chinese (Traditional)"),
    ("czech", "Čeština"),
    ("dansk", "Dansk"),
    ("eesti", "Eesti"),
    ("esperanto", "Esperanto"),
    ("farsi", "Farsi"),
    ("finnish", "Suomi"),
    ("galego", "Galego"),
    ("greek", "Ελληνικά"),
    ("hebrew", "Hebrew"),
    ("hindi", "Hindi"),
    ("hrvatski", "Hrvatski"),
    ("icelandic", "Íslenska"),
    ("islenska", "Íslenska"),
    ("japanese", "Japanese"),
    ("korean", "Korean"),
    ("latviesu", "Latviešu"),
    ("lietuviu", "Lietuvių"),
    ("magyar", "Magyar"),
    ("moldoveneste", "Moldovenească"),
    ("nederlands", "Nederlands"),
    ("norsk", "Norsk (bokmål)"),
    ("norsk-nynorsk", "Norsk (nynorsk)"),
    ("polski", "Polski"),
    ("romaneste", "Română"),
    ("russian", "Русский"),
    ("slovak", "Slovenčina"),
    ("slovenscina", "Slovenščina"),
    ("srpski", "Српски"),
    ("svenska", "Svenska"),
    ("tagalog", "Tagalog"),
    ("thai", "Thai"),
    ("turkce", "Türkçe"),
    ("ukrainian", "Українська"),
    ("vietnamese", "Tiếng Việt"),
    ("vlaams", "Vlaams"),
    ("walon", "Walon"),
];

/// O nome que aparece na lista de idiomas. Sem nome na tabela: o nome do
/// arquivo com a primeira letra maiúscula.
pub fn nome_idioma(arquivo: &str) -> String {
    let sem_ext = arquivo.strip_suffix(".lng").unwrap_or(arquivo);
    if let Some((_, nome)) = IDIOMAS.iter().find(|(k, _)| *k == sem_ext) {
        return (*nome).to_string();
    }
    let mut letras = sem_ext.chars();
    match letras.next() {
        Some(p) => p.to_uppercase().collect::<String>() + letras.as_str(),
        None => String::new(),
    }
}

/// Toda a interface: (chave, português, inglês). O teste confere que nenhuma
/// chave falta em nenhum dos dois idiomas.
pub const TEXTOS: &[(&str, &str, &str)] = &[
    ("escolha", "Escolha o sistema", "Choose the system"),
    (
        "escolha_sub",
        "Os dois funcionam no seu player. Você pode trocar quando quiser.",
        "Both work on your player. You can change it whenever you want.",
    ),
    (
        "system_desc",
        "O sistema de fábrica, melhorado.\nRádio, gravador, vídeo, Bluetooth.",
        "The factory system, improved.\nRadio, recorder, video, Bluetooth.",
    ),
    (
        "rockbox_desc",
        "Rockbox com o tema OpenPod.\nMais formatos, plugins e jogos.",
        "Rockbox with the OpenPod theme.\nMore formats, plugins and games.",
    ),
    ("site", "Versão mais nova do site", "Newest version from the site"),
    ("ou", "ou", "or"),
    ("arquivo", "Escolher arquivo…", "Choose file…"),
    ("restaurar", "Restaurar o original", "Restore the original"),
    ("continuar", "Continuar", "Continue"),
    ("conecte", "Conecte o player", "Connect the player"),
    ("passo1", "Desligue o player.", "Turn the player off."),
    ("passo2", "Segure o botão de baixo (play/pause).", "Hold the Down button (play/pause)."),
    ("passo3", "Ligue o cabo USB e solte o botão.", "Plug in the USB cable and release the button."),
    ("procurando", "Procurando o player…", "Looking for the player…"),
    ("encontrado", "Player encontrado · bateria {}%", "Player found · battery {}%"),
    ("instalar", "Instalar", "Install"),
    ("instalando_system", "Instalando o OpenPod System", "Installing OpenPod System"),
    ("instalando_rockbox", "Instalando o OpenPod RockBox", "Installing OpenPod RockBox"),
    ("nao_desconecte", "Não desconecte o cabo. Leva cerca de 2 minutos.", "Don't unplug the cable. It takes about 2 minutes."),
    ("etapa_backup", "Cópia de segurança da flash", "Full flash backup"),
    ("etapa_verify", "Conferência do aparelho", "Checking the player"),
    ("etapa_write", "Gravação", "Writing"),
    ("etapa_card", "Arquivos do cartão", "Card files"),
    ("pronto", "Pronto", "Done"),
    ("config_mantida", "Suas configurações foram mantidas.", "Your settings were kept."),
    (
        "recuperar_texto",
        "A instalação não terminou. Você pode restaurar a cópia de segurança feita antes de começar.",
        "The install did not finish. You can restore the backup made before starting.",
    ),
    ("copia_em", "Cópia de segurança: {}", "Backup: {}"),
    (
        "erro_reinicio",
        "O player não reiniciou no tempo. Se ele não voltar, restaure a cópia de segurança.",
        "The player did not restart in time. If it does not come back, restore the backup.",
    ),
    (
        "erro_nao_iniciou",
        "O player não iniciou o sistema novo. Restaure a cópia de segurança.",
        "The player did not start the new system. Restore the backup.",
    ),
    (
        "erro_copia_boot",
        "A cópia não tem o bootloader esperado. Nada foi gravado.",
        "The backup does not have the expected bootloader. Nothing was written.",
    ),
    (
        "erro_copia_tabela",
        "A cópia não tem a tabela de partições esperada. Nada foi gravado.",
        "The backup does not have the expected partition table. Nothing was written.",
    ),
    ("capa_beta", "* Capa do álbum: em beta", "* Album art: in beta"),
    (
        "erro_copia_outro",
        "Essa cópia é de outro player/modelo. Nada foi gravado.",
        "That backup is from another player/model. Nothing was written.",
    ),
    ("pronto_restaurado", "O original voltou para o player.", "The original is back on the player."),
    ("pronto_sub_restaurado", "Desconecte o cabo. O player está como veio de fábrica.", "Unplug the cable. The player is back to factory."),
    ("pronto_system", "O OpenPod System está instalado.", "OpenPod System is installed."),
    ("pronto_rockbox", "O OpenPod RockBox está instalado.", "OpenPod RockBox is installed."),
    (
        "pronto_sub_rockbox",
        "Desconecte o cabo. O tema OpenPod já vem escolhido.",
        "Unplug the cable. The OpenPod theme is already selected.",
    ),
    ("pronto_sub_system", "Desconecte o cabo. O sistema novo já está no player.", "Unplug the cable. The new system is on the player."),
    ("backup_em", "A cópia de segurança ficou em {}.", "The backup is in {}."),
    ("concluir", "Concluir", "Finish"),
    ("sem_site", "Sem conexão com o site. Escolha um arquivo.", "No connection to the site. Choose a file."),
    ("baixando", "Baixando o pacote…", "Downloading the package…"),
    ("bateria_baixa", "Bateria baixa. Carregue o player antes de instalar.", "Battery too low. Charge the player before installing."),
    ("restaurar_sem_copia", "Ainda não há cópia de segurança neste computador.", "There is no backup on this computer yet."),
    ("erro", "Não deu para continuar", "Could not continue"),
    ("tentar", "Tentar de novo", "Try again"),
    ("voltar", "Voltar", "Back"),
    ("arquivo_escolhido", "Arquivo escolhido:", "Chosen file:"),
    (
        "cartao_aviso",
        "O cartão do player vai receber o Rockbox e o tema OpenPod.",
        "The player's card will receive Rockbox and the OpenPod theme.",
    ),
    ("restaurando", "Restaurando o original", "Restoring the original"),
    (
        "restaurar_sub",
        "A cópia de segurança deste computador volta para o player.",
        "The backup from this computer goes back onto the player.",
    ),
    ("titulo_app", "Instalador OpenPod", "OpenPod Installer"),
    (
        "cartao_falta",
        "Sem cartão microSD no player. Insira um e pressione RESET.",
        "No microSD card in the player. Insert one and press RESET.",
    ),
    (
        "disco_falta",
        "O Rockbox não mostrou o disco USB. Se estiver em “só carregar”, mude para Mass Storage.",
        "Rockbox did not show its USB disk. If it is set to charge only, switch it to Mass Storage.",
    ),
    ("idioma_pt", "PT", "PT"),
    ("idioma_en", "EN", "EN"),
    ("idioma_player", "Idioma do player", "Player language"),
    (
        "apagar_rockbox",
        "Começar do zero: apagar as configurações e os dados do Rockbox no cartão",
        "Start fresh: erase the Rockbox settings and data on the card",
    ),
    ("apagar_titulo", "Começar do zero?", "Start fresh?"),
    (
        "apagar_texto",
        "Isto apaga a pasta .rockbox do cartão: configurações, temas e plugins que você adicionou, jogos salvos, favoritos e a base de músicas (o Rockbox refaz). Suas músicas e os outros arquivos do cartão não são tocados.",
        "This erases the .rockbox folder on the card: settings, themes and plugins you added, saved games, favourites and the music database (Rockbox rebuilds it). Your music and the other files on the card are not touched.",
    ),
    ("apagar_e_instalar", "Apagar e instalar", "Erase and install"),
    ("cancelar", "Cancelar", "Cancel"),
    (
        "pronto_comecou",
        "Começou do zero: as configurações antigas do Rockbox foram apagadas.",
        "Started fresh: the old Rockbox settings were erased.",
    ),
    ("erro_pacote", "O pacote não serve para este player.", "The package is not usable on this player."),
    ("erro_usb", "Não consegui falar com o player.", "Could not talk to the player."),
    ("erro_flash", "A gravação não deu certo.", "The write did not work."),
    ("erro_backup", "A cópia de segurança não deu certo.", "The backup did not work."),
    ("erro_cartao", "O cartão do player não deu certo.", "The player card did not work."),
    ("erro_interno", "Erro interno do instalador.", "Internal installer error."),
    ("erro_site", "Não consegui falar com o site.", "Could not reach the site."),
    ("erro_generico", "Não deu para continuar.", "Could not continue."),
    ("assinatura_invalida", "Este arquivo não foi feito pelo OpenPod", "This file was not made by OpenPod"),
    (
        "admin_aviso",
        "O macOS vai pedir a sua senha para o instalador conversar com o player.",
        "macOS will ask for your password so the installer can talk to the player.",
    ),
    (
        "admin_sem_senha",
        "Sem a senha o instalador não consegue falar com o player.",
        "Without the password the installer cannot talk to the player.",
    ),
    (
        "admin_trabalhador",
        "O instalador está conversando com o player como administrador.",
        "The installer is talking to the player as administrator.",
    ),
];

/// O texto de uma chave. Algumas levam `{}` para `format!`.
pub fn t(id: Idioma, chave: &str) -> &'static str {
    TEXTOS.iter().find(|(k, _, _)| *k == chave).map(|(_, pt, en)| if id == Idioma::Pt { *pt } else { *en }).unwrap_or("")
}

/// Uma mensagem de erro numa língua que o cliente entende. O detalhe
/// técnico (com o número E) vai para o diário, não para a tela.
pub fn erro(id: Idioma, s: &str) -> String {
    if s == crate::pacote::ERRO_ASSINATURA {
        return t(id, "assinatura_invalida").to_string();
    }
    if s.starts_with("E270") {
        return t(id, "bateria_baixa").to_string();
    }
    if s.starts_with("E435") {
        return t(id, "erro_copia_outro").to_string();
    }
    if s.starts_with("E428") {
        return t(id, "erro_copia_boot").to_string();
    }
    if s.starts_with("E429") {
        return t(id, "erro_copia_tabela").to_string();
    }
    if s.starts_with("E331") {
        return t(id, "erro_nao_iniciou").to_string();
    }
    if s.starts_with("E436") {
        return t(id, "erro_reinicio").to_string();
    }
    let chave = match s.as_bytes().get(1) {
        Some(b'1') => "erro_pacote",
        Some(b'2') => "erro_usb",
        Some(b'3') => "erro_flash",
        Some(b'4') => "erro_backup",
        Some(b'5') => "erro_cartao",
        Some(b'6') => "erro_site",
        Some(b'9') => "erro_interno",
        _ => "erro_generico",
    };
    t(id, chave).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nenhuma_chave_falta_nos_dois_idiomas() {
        let mut vistos = HashSet::new();
        for (chave, pt, en) in TEXTOS {
            assert!(vistos.insert(*chave), "chave repetida: {chave}");
            assert!(!chave.is_empty(), "chave vazia");
            assert!(!pt.trim().is_empty(), "{chave}: falta o português");
            assert!(!en.trim().is_empty(), "{chave}: falta o inglês");
            assert_eq!(t(Idioma::Pt, chave), *pt);
            assert_eq!(t(Idioma::En, chave), *en);
        }
        assert!(TEXTOS.len() > 40, "só {} chaves", TEXTOS.len());
    }

    /// Os nomes da lista de idiomas: o do próprio idioma, e o do arquivo
    /// com a primeira letra maiúscula quando não há nome na tabela.
    #[test]
    fn os_nomes_dos_idiomas_sao_legiveis() {
        assert_eq!(nome_idioma("portugues-brasileiro.lng"), "Português (Brasil)");
        assert_eq!(nome_idioma("portugues.lng"), "Português (Portugal)");
        assert_eq!(nome_idioma("english.lng"), "English (UK)");
        assert_eq!(nome_idioma("english-us.lng"), "English (US)");
        assert_eq!(nome_idioma("espanol.lng"), "Español");
        assert_eq!(nome_idioma("deutsch.lng"), "Deutsch");
        assert_eq!(nome_idioma("francais.lng"), "Français");
        assert_eq!(nome_idioma("italiano.lng"), "Italiano");
        // sem nome na tabela: o nome do arquivo com a primeira letra maiúscula
        assert_eq!(nome_idioma("nederlands.lng"), "Nederlands");
        assert_eq!(nome_idioma("tagalog.lng"), "Tagalog");
        assert_eq!(nome_idioma("qualquer-coisa.lng"), "Qualquer-coisa");
        // o nome do arquivo sem a extensão dá no mesmo
        assert_eq!(nome_idioma("deutsch"), "Deutsch");
    }

    #[test]
    fn a_tabela_de_idiomas_nao_repete_nome() {
        let mut vistos = HashSet::new();
        for (arquivo, nome) in IDIOMAS {
            assert!(vistos.insert(*arquivo), "idioma repetido: {arquivo}");
            assert!(!arquivo.is_empty());
            assert!(!nome.trim().is_empty(), "{arquivo}: falta o nome");
            assert_eq!(nome_idioma(&format!("{arquivo}.lng")), *nome);
        }
        assert!(IDIOMAS.len() > 40, "só {} idiomas na tabela", IDIOMAS.len());
    }

    #[test]
    fn os_erros_aparecem_na_lingua_certa() {
        assert_eq!(erro(Idioma::Pt, "E270: bateria baixa"), t(Idioma::Pt, "bateria_baixa"));
        assert_eq!(erro(Idioma::En, "E301: x"), t(Idioma::En, "erro_flash"));
        assert_eq!(erro(Idioma::En, crate::pacote::ERRO_ASSINATURA), t(Idioma::En, "assinatura_invalida"));
        assert_eq!(erro(Idioma::Pt, "E436: x"), t(Idioma::Pt, "erro_reinicio"));
        assert_eq!(erro(Idioma::En, "E331: x"), t(Idioma::En, "erro_nao_iniciou"));
        assert_eq!(erro(Idioma::En, "E428: x"), t(Idioma::En, "erro_copia_boot"));
        assert_eq!(erro(Idioma::En, "E429: x"), t(Idioma::En, "erro_copia_tabela"));
        assert!(!erro(Idioma::En, "qualquer coisa").is_empty());
    }
}
