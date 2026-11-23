//! As regras de gravação, sem USB: as que dá para testar em qualquer lugar.
//!
//! Nunca gravar abaixo de 0xE000 nem a partir de 0x1FC000. O setor 0xD000
//! (tabela) nunca é tocado. A conferência antes de gravar olha o bootloader,
//! a tabela e cada linha `expect` do pacote.

#![allow(dead_code)]

use super::{BOOT_SHA256, FLASH, GRAVAVEL, SETOR, TABELA_AUTOR, TABELA_GN438};
use crate::err;
use crate::pacote::{self, Expect, Tipo};

pub fn boot_ok(cur: &[u8]) -> bool {
    cur.len() >= 0xD000 && pacote::sha256(&cur[..0xD000]) == parse_hash(BOOT_SHA256)
}

pub fn tabela_ok(cur: &[u8], tipo: Tipo) -> bool {
    if cur.len() < 0xD000 + 0x1C {
        return false;
    }
    let h = pacote::sha256(&cur[0xD000..0xD000 + 0x1C]);
    h == parse_hash(TABELA_GN438) || (tipo == Tipo::Rockbox && h == parse_hash(TABELA_AUTOR))
}

pub fn expect_ok(cur: &[u8], expect: &[Expect]) -> bool {
    expect.iter().all(|e| {
        let (a, b) = (e.offset as usize, e.offset as usize + e.size as usize);
        b <= cur.len() && pacote::sha256(&cur[a..b]) == e.sha256
    })
}

/// Os setores da janela segura que diferem entre a flash atual e a desejada.
pub fn setores_diferentes(cur: &[u8], quer: &[u8]) -> Vec<u32> {
    let ss = SETOR as usize;
    (GRAVAVEL.0 / SETOR..GRAVAVEL.1 / SETOR)
        .filter(|&i| {
            let (a, b) = (i as usize * ss, (i as usize + 1) * ss);
            a + ss <= cur.len() && a + ss <= quer.len() && cur[a..b] != quer[a..b]
        })
        .collect()
}

/// A flash desejada: a atual com os pedaços do pacote por cima. Recusa
/// qualquer coisa fora da janela segura.
pub fn compor(cur: &[u8], segs: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, String> {
    if cur.len() < FLASH as usize {
        return Err(err!(E156, "a flash atual está curta demais"));
    }
    let mut img = cur.to_vec();
    for (offset, dados) in segs {
        let inicio = *offset as usize;
        let fim = inicio + dados.len().div_ceil(SETOR as usize) * SETOR as usize;
        if inicio < GRAVAVEL.0 as usize || fim > GRAVAVEL.1 as usize || inicio % SETOR as usize != 0 {
            return Err(err!(
                E154,
                "o pacote grava 0x{inicio:06x}..0x{fim:06x}, fora da janela segura 0x{:06x}..0x{:06x}",
                GRAVAVEL.0,
                GRAVAVEL.1
            ));
        }
        img[inicio..inicio + dados.len()].copy_from_slice(dados);
        img[inicio + dados.len()..fim].fill(0xff);
    }
    Ok(img)
}

/// A cópia de segurança tem de ser deste mesmo player: a base até 0xE000
/// (bootloader e tabela) precisa ser idêntica. Senão, nada é gravado.
pub fn conferir_copia(cur: &[u8], img: &[u8]) -> Result<(), String> {
    if cur.len() < 0xE000 || img.len() < 0xE000 || cur[..0xE000] != img[..0xE000] {
        return Err(err!(E435, "essa cópia é de outro player/modelo; nada foi gravado"));
    }
    Ok(())
}

/// Restaurar exige a cópia deste player **e** o bootloader e a tabela
/// esperados (GN-438, ou o player do autor no RockBox).
pub fn conferir_restauro(cur: &[u8], img: &[u8]) -> Result<(), String> {
    conferir_copia(cur, img)?;
    if !boot_ok(img) {
        return Err(err!(E428, "a cópia não tem o bootloader esperado; nada foi gravado"));
    }
    if !tabela_ok(img, Tipo::Rockbox) {
        return Err(err!(E429, "a cópia não tem a tabela de partições esperada; nada foi gravado"));
    }
    Ok(())
}

fn parse_hash(s: &str) -> [u8; 32] {
    pacote::unhex32(s).expect("hash de regra inválido")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacote::{sha256, unhex32};

    fn flash() -> Vec<u8> {
        vec![0xffu8; FLASH as usize]
    }

    #[test]
    fn so_os_setores_que_mudam() {
        let cur = flash();
        let mut quer = cur.clone();
        let ss = SETOR as usize;
        quer[0xE000 + 5] = 0x11;
        quer[0x12000 + ss - 1] = 0x22;
        assert_eq!(setores_diferentes(&cur, &quer), vec![0xE000 / SETOR, 0x12000 / SETOR]);
        assert!(setores_diferentes(&cur, &cur).is_empty());
    }

    #[test]
    fn compor_cobre_e_arredonda_para_o_setor() {
        let cur = flash();
        let segs = vec![(0xE000u32, vec![0xAA; 3])];
        let img = compor(&cur, &segs).unwrap();
        assert_eq!(&img[0xE000..0xE003], &[0xAA, 0xAA, 0xAA]);
        assert!(img[0xE003..0xE000 + SETOR as usize].iter().all(|&b| b == 0xff));
        assert_eq!(&img[..0xE000], &cur[..0xE000]);
    }

    #[test]
    fn recusa_endereco_fora_da_faixa() {
        let cur = flash();
        for off in [0u32, 0xD000, 0xDC00] {
            let e = compor(&cur, &[(off, vec![1; SETOR as usize])]).unwrap_err();
            assert!(e.starts_with("E154"), "{e}");
        }
        let e = compor(&cur, &[(GRAVAVEL.1, vec![1; SETOR as usize])]).unwrap_err();
        assert!(e.starts_with("E154"), "{e}");
        let e = compor(&cur, &[(0xE800, vec![1; 8])]).unwrap_err();
        assert!(e.starts_with("E154"), "desalinhado: {e}");
        // do fim da janela para trás cabe
        let ultimo = GRAVAVEL.1 - SETOR;
        assert!(compor(&cur, &[(ultimo, vec![1; SETOR as usize])]).is_ok());
    }

    fn expect(offset: u32, dados: &[u8]) -> Expect {
        Expect { offset, size: dados.len() as u32, sha256: sha256(dados) }
    }

    #[test]
    fn expect_certo_e_errado() {
        let mut cur = flash();
        cur[0x10000..0x10008].copy_from_slice(b"GN-438!!");
        let bom = expect(0x10000, b"GN-438!!");
        let ruim = expect(0x10000, b"outra!!!");
        assert!(expect_ok(&cur, &[bom.clone()]));
        assert!(!expect_ok(&cur, &[ruim]));
        assert!(!expect_ok(&cur, &[expect(FLASH - 2, b"abc")]), "passa do fim");
        assert!(expect_ok(&cur, &[]));
    }

    #[test]
    fn restaurar_recusa_copia_de_outro_player() {
        let mut cur = flash();
        cur[..0xE000].fill(0x11);
        let mut img = cur.clone();
        assert!(conferir_copia(&cur, &img).is_ok());
        // a parte gravável pode diferir: é o que se restaura
        img[0xE000] ^= 1;
        assert!(conferir_copia(&cur, &img).is_ok());
        // a base mudou (outro player/modelo)
        img[0x8000] ^= 1;
        assert!(conferir_copia(&cur, &img).unwrap_err().starts_with("E435"));
        assert!(conferir_copia(&cur, &img[..0x8000]).unwrap_err().starts_with("E435"));
    }

    fn kit() -> Vec<u8> {
        let dados = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/openpod/kit-teste-3.5-diag.up")).unwrap();
        dados[0x100..].to_vec()
    }

    #[test]
    fn restauro_exige_bootloader_e_tabela_do_gn438() {
        let base = kit();
        assert!(conferir_restauro(&base, &base).is_ok());
        // a parte gravável pode diferir
        let mut img = base.clone();
        img[0xE000] ^= 1;
        assert!(conferir_restauro(&base, &img).is_ok());
        // outra base: outro player/modelo
        let mut img = base.clone();
        img[0x8000] ^= 1;
        assert!(conferir_restauro(&base, &img).unwrap_err().starts_with("E435"));
        // mesma base, mas sem o bootloader esperado
        let mut cur = base.clone();
        let mut img = base.clone();
        cur[0x100] ^= 1;
        img[0x100] ^= 1;
        assert!(conferir_restauro(&cur, &img).unwrap_err().starts_with("E428"));
        // mesma base, mas sem a tabela do GN-438
        let mut cur = base.clone();
        let mut img = base.clone();
        cur[0xD010] ^= 1;
        img[0xD010] ^= 1;
        assert!(conferir_restauro(&cur, &img).unwrap_err().starts_with("E429"));
    }

    #[test]
    fn bootloader_e_tabela_errados_sao_recusados() {
        let cur = flash();
        assert!(!boot_ok(&cur));
        assert!(!tabela_ok(&cur, Tipo::Rockbox));
        assert!(!boot_ok(&cur[..0x100]));
        assert!(unhex32(BOOT_SHA256).is_some());
        assert!(unhex32(TABELA_GN438).is_some());
        assert!(unhex32(TABELA_AUTOR).is_some());
    }
}
