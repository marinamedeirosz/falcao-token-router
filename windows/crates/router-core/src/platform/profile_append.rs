//! Acrescentar um bloco ao perfil de um shell **em bytes**, sem nunca reescrever
//! o arquivo.
//!
//! No macOS um `~/.zshrc` que não decodificava em UTF-8 virava "vazio" e era
//! sobrescrito só com a linha nova — o arquivo inteiro do usuário, perdido. O
//! Windows tem mais jeitos de um perfil não ser UTF-8: o Windows PowerShell 5.1
//! grava "Unicode" (UTF-16LE com BOM), editores antigos gravam ANSI. Então:
//!
//! - a codificação é detectada pelo BOM e o bloco é codificado nela (UTF-16LE,
//!   UTF-16BE, UTF-8); sem BOM, o bloco é ASCII — igual em UTF-8 e em ANSI;
//! - o fim de linha é o que o arquivo já usa (CRLF ou LF);
//! - o arquivo é aberto em modo append: nada do que existia é tocado, e um link
//!   para o perfil real é seguido, não substituído;
//! - arquivo que existe e não pode ser lido faz a operação falhar — nunca vira
//!   "vazio".
//!
//! A remoção ([`remove_block`]) é a única que reescreve, e mesmo ela só APAGA
//! faixas de bytes: o que fica sai igual ao que entrou, em qualquer codificação.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;

use super::atomic_write::{read_retrying, retrying};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppendOutcome {
    Added,
    AlreadyPresent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemoveOutcome {
    Removed,
    NotPresent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Encoding {
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// Sem BOM: UTF-8 ou ANSI. O bloco é ASCII, igual nos dois.
    Plain,
}

fn detect(bytes: &[u8]) -> (Encoding, &[u8]) {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => (Encoding::Utf8Bom, rest),
        [0xFF, 0xFE, rest @ ..] => (Encoding::Utf16Le, rest),
        [0xFE, 0xFF, rest @ ..] => (Encoding::Utf16Be, rest),
        _ => (Encoding::Plain, bytes),
    }
}

fn decode(encoding: Encoding, body: &[u8]) -> String {
    let units = |be: bool| -> Vec<u16> {
        body.as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                if be {
                    u16::from_be_bytes(*pair)
                } else {
                    u16::from_le_bytes(*pair)
                }
            })
            .collect()
    };
    match encoding {
        Encoding::Utf16Le => String::from_utf16_lossy(&units(false)),
        Encoding::Utf16Be => String::from_utf16_lossy(&units(true)),
        // ANSI decodificado como UTF-8 com perda: os bytes não-ASCII viram �,
        // mas o que se procura (o marcador) é ASCII e sobrevive.
        Encoding::Utf8Bom | Encoding::Plain => String::from_utf8_lossy(body).into_owned(),
    }
}

fn encode(encoding: Encoding, text: &str) -> Vec<u8> {
    match encoding {
        Encoding::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        Encoding::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        Encoding::Utf8Bom | Encoding::Plain => text.as_bytes().to_vec(),
    }
}

/// O texto de um perfil, seja qual for a codificação em que ele foi salvo.
///
/// **Esta é a única leitura de BOM que se deve usar.** Havia três cópias dela no
/// repositório e só esta trata UTF-16**BE**: num perfil salvo assim, as outras
/// duas não achavam o marcador e o botão "Ativar" nunca ficava verde, por mais
/// que o usuário clicasse.
pub fn decode_profile(bytes: &[u8]) -> String {
    let (encoding, body) = detect(bytes);
    decode(encoding, body)
}

/// Acrescenta `lines` ao arquivo se `marker` ainda não está nele (sem caixa).
/// Arquivo ausente é criado (com a pasta), em ASCII, com o fim de linha
/// `default_eol`.
pub fn append_block(
    path: &Path,
    marker: &str,
    lines: &[&str],
    default_eol: &str,
) -> io::Result<AppendOutcome> {
    let existing = match read_retrying(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };

    let Some(bytes) = existing else {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut block = lines.join(default_eol);
        block.push_str(default_eol);
        fs::write(path, block.as_bytes())?;
        return Ok(AppendOutcome::Added);
    };

    let (encoding, body) = detect(&bytes);
    let text = decode(encoding, body);
    if text.to_lowercase().contains(&marker.to_lowercase()) {
        return Ok(AppendOutcome::AlreadyPresent);
    }
    let eol = if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\n') {
        "\n"
    } else {
        default_eol
    };

    let mut block = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        block.push_str(eol); // a última linha do usuário não gruda no bloco
    }
    block.push_str(eol);
    block.push_str(&lines.join(eol));
    block.push_str(eol);

    let payload = encode(encoding, &block);
    let mut file = retrying(|| OpenOptions::new().append(true).open(path))?;
    file.write_all(&payload)?;
    file.sync_all()?;
    Ok(AppendOutcome::Added)
}

/// Tira do arquivo as linhas do nosso bloco (as que citam `marker`, e o
/// comentário imediatamente antes), preservando o resto na codificação original.
///
/// O `append_block` ganhava de graça uma garantia que a remoção não tem: abrindo
/// em modo append, ele **segue** um `$PROFILE` que é link simbólico em vez de
/// trocá-lo. Por isso a remoção grava **no lugar** (truncate + write) e nunca
/// por `write_atomic`: o rename por cima substituiria o link pelo arquivo novo.
/// Arquivo ilegível continua sendo erro, nunca "vazio".
pub fn remove_block(path: &Path, marker: &str) -> io::Result<RemoveOutcome> {
    let bytes = match read_retrying(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(RemoveOutcome::NotPresent),
        Err(e) => return Err(e),
    };

    let (encoding, body) = detect(&bytes);
    let needle = marker.to_lowercase();
    if !decode(encoding, body).to_lowercase().contains(&needle) {
        return Ok(RemoveOutcome::NotPresent);
    }
    let line = |range: &Range<usize>| decode(encoding, &body[range.clone()]);

    let mut kept: Vec<Range<usize>> = Vec::new();
    for range in line_ranges(encoding, body) {
        if !line(&range).to_lowercase().contains(&needle) {
            kept.push(range);
            continue;
        }
        // O comentário do bloco sai junto com a linha, e com ele a linha em
        // branco que o `append_block` põe antes — só ela: as do usuário ficam.
        if matches!(kept.last(), Some(r) if is_block_comment(&line(r))) {
            kept.pop();
            if matches!(kept.last(), Some(r) if line(r).trim().is_empty()) {
                kept.pop();
            }
        }
    }

    // O BOM é o que `detect` tirou da frente e continua onde estava.
    let mut out = bytes[..bytes.len() - body.len()].to_vec();
    for range in kept {
        out.extend_from_slice(&body[range]);
    }

    let mut file = retrying(|| OpenOptions::new().write(true).truncate(true).open(path))?;
    file.write_all(&out)?;
    file.sync_all()?;
    Ok(RemoveOutcome::Removed)
}

/// As linhas do corpo em FAIXAS DE BYTES, cada uma com o próprio fim de linha
/// (daí o CRLF ou LF do arquivo sobreviver sem ninguém decidir nada).
///
/// Cortar por byte, e não pelo texto decodificado, é o que preserva o que não é
/// ASCII: um perfil ANSI com "café" decodifica COM PERDA (o 0xE9 vira �) e
/// reescrevê-lo a partir do texto estragaria o arquivo do usuário.
fn line_ranges(encoding: Encoding, body: &[u8]) -> Vec<Range<usize>> {
    let newline: &[u8] = match encoding {
        Encoding::Utf16Le => &[0x0A, 0x00],
        Encoding::Utf16Be => &[0x00, 0x0A],
        Encoding::Utf8Bom | Encoding::Plain => &[0x0A],
    };
    let unit = newline.len();

    let mut ranges = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at + unit <= body.len() {
        if &body[at..at + unit] == newline {
            ranges.push(start..at + unit);
            start = at + unit;
        }
        at += unit;
    }
    if start < body.len() {
        ranges.push(start..body.len()); // a última linha, sem quebra no fim
    }
    ranges
}

/// A linha é o comentário que acompanha o nosso bloco? Ele é ASCII e nomeia o
/// produto (`# Falcao Router - ...`), então comentário do usuário não some.
fn is_block_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('#') && line.to_lowercase().contains("falcao")
}
