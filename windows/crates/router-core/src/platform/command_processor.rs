//! O `AutoRun` do Prompt de Comando — o espelho do `profile_append` para o
//! registro.
//!
//! O `cmd.exe` não tem `$PROFILE`. O único gancho nativo é
//! `HKCU\Software\Microsoft\Command Processor\AutoRun`, uma linha de comando que
//! ele roda antes do primeiro prompt; é ali que a macro `doskey claude` nasce.
//!
//! Duas diferenças em relação ao arquivo de perfil, e as duas simplificam:
//!
//! - o registro é **UTF-16 nativo**, então não há dança de codificação: um nome
//!   de usuário com acento entra inteiro, sem o cuidado de manter o bloco ASCII
//!   que o `profile_append` precisa ter;
//! - o valor é **um só**, compartilhado com quem já estava lá. Clink, ConEmu e
//!   Anaconda põem o deles aqui — por isso a instalação **compõe** com `&`, e
//!   nunca sobrescreve. Atropelar o valor alheio quebraria o terminal do usuário
//!   sem ele entender por quê.
//!
//! A chave é injetável (`CommandProcessorKey`), como `EditionEnv` e `GitBashEnv`:
//! os testes escrevem sob uma subchave própria e limpam, sem tocar o AutoRun da
//! máquina de quem roda a suíte.

use std::io;

/// A subchave do `HKEY_CURRENT_USER` onde o `AutoRun` mora. Injetável para os
/// testes não mexerem no AutoRun de verdade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandProcessorKey {
    pub subkey: String,
}

impl CommandProcessorKey {
    /// A de verdade — a que o `cmd.exe` lê.
    pub fn current_user() -> Self {
        CommandProcessorKey {
            subkey: r"Software\Microsoft\Command Processor".to_string(),
        }
    }

    /// Uma subchave de teste sob `HKCU\Software`, para a suíte não tocar a real.
    pub fn for_tests(name: &str) -> Self {
        CommandProcessorKey {
            subkey: format!(r"Software\FalcaoRouterTests\{name}"),
        }
    }
}

/// O que a instalação fez — o mesmo vocabulário do `profile_append`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstallOutcome {
    Added,
    AlreadyPresent,
}

/// O que a remoção fez.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemoveOutcome {
    Removed,
    NotPresent,
}

/// O valor cru do `AutoRun`, sem expandir variável (`RRF_NOEXPAND`): o que sai
/// daqui tem de poder voltar igual.
///
/// TRILHA T1: implementar com `RegGetValueW`, no padrão de `platform/links.rs`.
pub fn autorun(_key: &CommandProcessorKey) -> Option<String> {
    None
}

/// Acrescenta `call` ao `AutoRun`, **compondo** com o que já estiver lá.
///
/// TRILHA T1: vazio → só o nosso; existente sem o marcador → `"<dele> & <nosso>"`;
/// com o marcador → `AlreadyPresent`.
pub fn install(
    _key: &CommandProcessorKey,
    _call: &str,
    _marker: &str,
) -> io::Result<InstallOutcome> {
    Ok(InstallOutcome::AlreadyPresent)
}

/// Tira só o NOSSO segmento do `AutoRun`, preservando o de terceiros.
///
/// TRILHA T1: divide por `&`, descarta os segmentos com o marcador, regrava; sem
/// sobrar nada, apaga o valor.
pub fn remove(_key: &CommandProcessorKey, _marker: &str) -> io::Result<RemoveOutcome> {
    Ok(RemoveOutcome::NotPresent)
}

/// O `AutoRun` já carrega o nosso script?
pub fn has_marker(key: &CommandProcessorKey, marker: &str) -> bool {
    autorun(key).is_some_and(|value| value.to_lowercase().contains(&marker.to_lowercase()))
}
