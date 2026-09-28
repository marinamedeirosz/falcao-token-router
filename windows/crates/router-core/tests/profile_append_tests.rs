//! Tirar do perfil do shell o bloco que a integração pôs lá.
//!
//! Até aqui o produto só sabia INSTALAR: apagar a linha era um passo manual no
//! README. O teste central é o ida-e-volta em BYTES — `append_block` seguido de
//! `remove_block` tem de devolver o arquivo idêntico ao que era, nas quatro
//! codificações que um `$PROFILE` do Windows aparece (o 5.1 grava UTF-16LE, os
//! editores antigos gravam ANSI) e nos dois fins de linha.
//!
//! O episódio que paira sobre tudo isto é o `~/.zshrc` do macOS: perfil que não
//! decodificava virava "vazio" e era sobrescrito só com a linha nova.

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use router_core::platform::links::{is_symlink, symlink_file};
use router_core::platform::profile_append::{append_block, remove_block, RemoveOutcome};

/// A forma do comentário que o `ensure_in_profile` acrescenta (o `PROFILE_COMMENT`
/// de `engine::shell_integration`, que é privado).
const COMMENT: &str = "# Falcao Router - integracao de terminal (claude <grupo>)";

/// A linha carregadora, que é também o marcador.
const LINE: &str = r#"if (Test-Path "$env:LOCALAPPDATA\com.synqo.falcao-router\shell.ps1") { . "$env:LOCALAPPDATA\com.synqo.falcao-router\shell.ps1" }"#;

#[derive(Clone, Copy, Debug)]
enum Enc {
    Plain,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn encoded(enc: Enc, text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    match enc {
        Enc::Plain => out.extend_from_slice(text.as_bytes()),
        Enc::Utf8Bom => {
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            out.extend_from_slice(text.as_bytes());
        }
        Enc::Utf16Le => {
            out.extend_from_slice(&[0xFF, 0xFE]);
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        }
        Enc::Utf16Be => {
            out.extend_from_slice(&[0xFE, 0xFF]);
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        }
    }
    out
}

fn install(profile: &Path, eol: &str) {
    append_block(profile, LINE, &[COMMENT, LINE], eol).unwrap();
}

/// O que a instalação escreveu sai inteiro, e mais nada: o arquivo volta a ser
/// byte a byte o que era — em UTF-8 com BOM, UTF-16LE, UTF-16BE e sem BOM, com
/// CRLF e com LF.
#[test]
fn append_then_remove_gives_back_every_original_byte() {
    for enc in [Enc::Plain, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be] {
        for eol in ["\r\n", "\n"] {
            let tmp = tempfile::tempdir().unwrap();
            let profile = tmp.path().join("perfil.ps1");
            // Com linha em branco e comentário do usuário colados no bloco: é
            // aí que uma remoção afobada come o que não é dela.
            let original = encoded(
                enc,
                &format!("# meu perfil{eol}Set-Alias ll Get-ChildItem{eol}{eol}# fim{eol}"),
            );
            fs::write(&profile, &original).unwrap();

            install(&profile, eol);
            assert_ne!(
                fs::read(&profile).unwrap(),
                original,
                "{enc:?} não instalou"
            );

            let out = remove_block(&profile, LINE).unwrap();

            assert_eq!(out, RemoveOutcome::Removed, "{enc:?} {eol:?}");
            assert_eq!(fs::read(&profile).unwrap(), original, "{enc:?} {eol:?}");
        }
    }
}

/// Perfil salvo em ANSI: o 0xE9 ("é" em Windows-1252) não sobrevive a um
/// decodifica-edita-regrava, só ao corte por byte.
#[test]
fn an_ansi_profile_keeps_the_bytes_that_are_not_utf8() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("perfil.ps1");
    let mut original = b"Set-Alias caf".to_vec();
    original.push(0xE9);
    original.extend_from_slice(b" Get-Date\r\n");
    fs::write(&profile, &original).unwrap();

    install(&profile, "\r\n");
    remove_block(&profile, LINE).unwrap();

    assert_eq!(fs::read(&profile).unwrap(), original);
}

/// O `$PROFILE` pode ser um link para o perfil de verdade (é assim que se
/// compartilha um perfil entre máquinas). Gravar por `write_atomic` — temporário
/// + rename — trocaria o link pelo arquivo novo e deixaria o perfil órfão.
#[test]
fn a_symlinked_profile_is_still_a_symlink_after_the_removal() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("perfil-de-verdade.ps1");
    let link = tmp.path().join("Microsoft.PowerShell_profile.ps1");
    fs::write(&real, "# meu perfil\r\n").unwrap();
    if let Err(e) = symlink_file(&real, &link) {
        // Sem Developer Mode (nem admin) o Windows recusa com 1314; nesta
        // máquina o teste não tem o que provar.
        eprintln!("pulado: symlink de arquivo indisponível ({e})");
        return;
    }

    install(&link, "\r\n");
    remove_block(&link, LINE).unwrap();

    assert!(is_symlink(&link), "o link virou arquivo comum");
    assert_eq!(fs::read_to_string(&real).unwrap(), "# meu perfil\r\n");
}

/// Sem o marcador (ou sem o arquivo) nada é tocado — nem o mtime.
#[test]
fn a_profile_without_the_marker_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("perfil.ps1");
    let original = b"# perfil de outra pessoa\r\n";
    fs::write(&profile, original).unwrap();

    assert_eq!(
        remove_block(&profile, LINE).unwrap(),
        RemoveOutcome::NotPresent
    );
    assert_eq!(
        remove_block(&tmp.path().join("nao-existe.ps1"), LINE).unwrap(),
        RemoveOutcome::NotPresent
    );
    assert_eq!(fs::read(&profile).unwrap(), original);
}

/// Desativar duas vezes (o botão e o desinstalador, por exemplo) não muda nada
/// na segunda.
#[test]
fn removing_twice_changes_nothing_the_second_time() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join(".bashrc");
    fs::write(&profile, "alias ll='ls -l'\n").unwrap();
    install(&profile, "\n");

    assert_eq!(
        remove_block(&profile, LINE).unwrap(),
        RemoveOutcome::Removed
    );
    let after = fs::read(&profile).unwrap();
    assert_eq!(
        remove_block(&profile, LINE).unwrap(),
        RemoveOutcome::NotPresent
    );

    assert_eq!(after, b"alias ll='ls -l'\n");
    assert_eq!(fs::read(&profile).unwrap(), after);
}

/// Perfil preso por outro processo: erro, nunca "vazio" (o episódio do
/// `~/.zshrc`). O arquivo fica como estava.
#[test]
fn a_locked_profile_is_an_error_and_never_becomes_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("perfil.ps1");
    fs::write(&profile, "# meu\r\n").unwrap();
    install(&profile, "\r\n");
    let original = fs::read(&profile).unwrap();

    let held = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&profile)
        .unwrap();
    let result = remove_block(&profile, LINE);
    drop(held);

    assert!(result.is_err(), "{result:?}");
    assert_eq!(fs::read(&profile).unwrap(), original);
}
