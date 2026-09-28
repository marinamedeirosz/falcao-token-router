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

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE, RRF_NOEXPAND,
    RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

/// O nome do valor sob a chave — o que o `cmd.exe` lê antes do primeiro prompt.
const VALUE_NAME: &str = "AutoRun";

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

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Comparação sem caixa, como a do `profile_append`: o caminho do nosso script
/// volta do registro com a caixa que o usuário digitou, não com a que gravamos.
fn contains_marker(text: &str, marker: &str) -> bool {
    text.to_lowercase().contains(&marker.to_lowercase())
}

/// Uma chave aberta que se fecha sozinha ao sair de escopo.
struct OpenKey(HKEY);

impl Drop for OpenKey {
    fn drop(&mut self) {
        // SAFETY: o handle é nosso, foi aberto com sucesso e não será mais usado.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Abre a chave para escrita, criando-a se não existir. Criar é o que dispensa
/// pré-requisito: a chave real já existe em qualquer Windows, a de teste não.
fn open_for_write(key: &CommandProcessorKey) -> io::Result<OpenKey> {
    let subkey = wide(&key.subkey);
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: `subkey` termina em zero e vive até o fim da chamada; classe,
    // atributos de segurança e disposição são opcionais (nulos); `handle` é
    // nosso e recebe a chave aberta.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        )
    };
    if status == 0 {
        Ok(OpenKey(handle))
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// O valor cru **e o tipo em que ele está gravado**.
///
/// O tipo volta junto porque regravar como `REG_SZ` um `REG_EXPAND_SZ` de
/// terceiro mataria o `%VAR%` dele — é o mesmo estrago de sobrescrever o valor,
/// só mais difícil de enxergar. `RRF_NOEXPAND` é o outro lado disso: sem ele o
/// Windows devolveria o valor JÁ expandido, e a regravação congelaria em texto
/// o que era uma variável.
fn read_raw(key: &CommandProcessorKey) -> Option<(String, REG_VALUE_TYPE)> {
    let subkey = wide(&key.subkey);
    let name = wide(VALUE_NAME);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut kind: REG_VALUE_TYPE = 0;
    let mut size: u32 = 0;
    // SAFETY: chave e nome terminam em zero e vivem até o fim da chamada; com
    // `pvdata` nulo a chamada só devolve em `size` quantos bytes o valor ocupa.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    if size == 0 {
        return Some((String::new(), kind));
    }

    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buffer` tem os bytes que a chamada anterior pediu, e `size` diz
    // quantos são; o que sai é UTF-16 nativo, sem conversão no caminho.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let units = (size as usize / 2).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..units]);
    // O terminador entra no tamanho que o registro informa; ele não é texto.
    Some((text.trim_end_matches('\0').to_string(), kind))
}

fn write_value(key: &CommandProcessorKey, value: &str, kind: REG_VALUE_TYPE) -> io::Result<()> {
    let open = open_for_write(key)?;
    let name = wide(VALUE_NAME);
    let data = wide(value);
    let bytes = (data.len() * 2) as u32;
    // SAFETY: a chave está aberta com KEY_WRITE; nome e dado terminam em zero e
    // vivem até o fim da chamada; `bytes` é o tamanho real de `data`, com o
    // terminador que o registro espera numa string.
    let status =
        unsafe { RegSetValueExW(open.0, name.as_ptr(), 0, kind, data.as_ptr().cast(), bytes) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

fn delete_value(key: &CommandProcessorKey) -> io::Result<()> {
    let open = open_for_write(key)?;
    let name = wide(VALUE_NAME);
    // SAFETY: a chave está aberta com KEY_WRITE e o nome termina em zero.
    let status = unsafe { RegDeleteValueW(open.0, name.as_ptr()) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// Quebra o valor em pares (separador que veio ANTES, segmento).
///
/// O separador volta como estava porque no `cmd` ele não é enfeite: `&&` é "só
/// se o anterior deu certo" e `&` é "de qualquer jeito". Normalizar um no outro
/// mudaria o que o AutoRun de terceiro faz sem ele ter tocado em nada.
fn segments(value: &str) -> Vec<(&str, &str)> {
    let mut parts = Vec::new();
    let bytes = value.as_bytes();
    let (mut separator, mut start, mut i) = ("", 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'&' {
            i += 1;
            continue;
        }
        let run = i;
        while i < bytes.len() && bytes[i] == b'&' {
            i += 1;
        }
        parts.push((separator, &value[start..run]));
        separator = &value[run..i];
        start = i;
    }
    parts.push((separator, &value[start..]));
    parts
}

/// O valor cru do `AutoRun`, sem expandir variável (`RRF_NOEXPAND`): o que sai
/// daqui tem de poder voltar igual.
pub fn autorun(key: &CommandProcessorKey) -> Option<String> {
    read_raw(key).map(|(value, _)| value)
}

/// Acrescenta `call` ao `AutoRun`, **compondo** com o que já estiver lá.
pub fn install(key: &CommandProcessorKey, call: &str, marker: &str) -> io::Result<InstallOutcome> {
    let current = read_raw(key);
    if current
        .as_ref()
        .is_some_and(|(value, _)| contains_marker(value, marker))
    {
        return Ok(InstallOutcome::AlreadyPresent);
    }
    // Valor ausente conta como vazio, e o tipo de um valor novo é `REG_SZ`: o
    // nosso caminho é absoluto, não tem `%VAR%` para expandir.
    let (existing, kind) = current.unwrap_or_else(|| (String::new(), REG_SZ));
    let existing = existing.trim();

    let composed = if existing.is_empty() {
        call.to_string()
    } else {
        // `&`, nunca `&&`: o AutoRun de terceiro pode sair com código diferente
        // de zero, e com `&&` a nossa macro simplesmente não nasceria — o
        // `claude <grupo>` voltaria a cair no `claude.exe` do PATH sem sintoma.
        format!("{existing} & {call}")
    };
    write_value(key, &composed, kind)?;
    Ok(InstallOutcome::Added)
}

/// Tira só o NOSSO segmento do `AutoRun`, preservando o de terceiros.
pub fn remove(key: &CommandProcessorKey, marker: &str) -> io::Result<RemoveOutcome> {
    let Some((value, kind)) = read_raw(key) else {
        return Ok(RemoveOutcome::NotPresent);
    };
    if !contains_marker(&value, marker) {
        return Ok(RemoveOutcome::NotPresent);
    }

    // Segmento em branco também sai: ele não é comando de ninguém, e sobrando na
    // frente viraria um `& ...` — sintaxe inválida no `cmd`, num valor que roda
    // antes de todo prompt.
    let kept: Vec<(&str, &str)> = segments(&value)
        .into_iter()
        .filter(|(_, segment)| !segment.trim().is_empty() && !contains_marker(segment, marker))
        .collect();

    let mut rebuilt = String::new();
    for (index, (separator, segment)) in kept.iter().enumerate() {
        if index > 0 {
            rebuilt.push_str(separator);
        }
        rebuilt.push_str(segment);
    }

    let rebuilt = rebuilt.trim();
    if rebuilt.is_empty() {
        // Sem sobrar nada o valor SAI, e não vira string vazia: um `AutoRun`
        // vazio ainda é um `AutoRun`, e o `cmd` reclama dele a cada prompt.
        delete_value(key)?;
    } else {
        write_value(key, rebuilt, kind)?;
    }
    Ok(RemoveOutcome::Removed)
}

/// O `AutoRun` já carrega o nosso script?
pub fn has_marker(key: &CommandProcessorKey, marker: &str) -> bool {
    autorun(key).is_some_and(|value| contains_marker(&value, marker))
}
