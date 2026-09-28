//! A árvore de processos vista de baixo: quem é o pai, e o pai do pai.
//!
//! Existe por um motivo só, e é o mais caro que este porte já pagou: **nada no
//! produto sabia em que shell o usuário estava**. O `router doctor` conferia os
//! dois `$PROFILE` e o `.bashrc`, dizia "tudo certo", e o usuário que digitava
//! `claude <grupo>` no Prompt de Comando continuava caindo no `claude` puro — o
//! único caso que o diagnóstico não enxergava era justamente o dele.
//!
//! A enumeração `(pid, ppid, exe)` por Toolhelp já existia no repositório, presa
//! dentro do `#[cfg(test)] mod test_support` do `job.rs`. Aqui ela vira
//! produção, e o `test_support` passa a chamá-la — nada de segunda implementação.

use std::collections::{HashMap, HashSet};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// Teto da subida. Ninguém aninha terminal mais que isto, e um teto pequeno é o
/// que impede a foto não-atômica do Toolhelp de virar laço no `doctor`.
const MAX_DEPTH: usize = 12;

/// Um processo vivo, como a foto do Toolhelp o descreve.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub pid: u32,
    pub parent: u32,
    /// O nome do executável, sem caminho (`cmd.exe`).
    pub exe: String,
}

/// O shell em que o usuário está de fato digitando.
///
/// Não é o `ShellKind` do `engine`: esta camada não conhece aquela, e o
/// mapeamento é de quem está por cima.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParentShell {
    Cmd,
    WindowsPowerShell,
    PowerShell7,
    Bash,
}

/// `(pid, pid do pai, nome do exe)` de cada processo vivo.
pub fn processes() -> Vec<Entry> {
    // SAFETY: foto da lista de processos; o handle é fechado no fim.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut all = Vec::new();
    // SAFETY: `entry` é nosso, com o `dwSize` preenchido.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        all.push(Entry {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
        });
        // SAFETY: idem.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: o handle é nosso.
    unsafe { CloseHandle(snapshot) };
    all
}

/// A cadeia de ancestrais de um pid, do pai para cima.
///
/// A parada é garantida por teto E por conjunto de visitados — os dois, não um.
/// A foto do Toolhelp não é atômica: entre uma entrada e outra um pid pode ter
/// sido reciclado e apontar para trás, fechando ciclo. Sem isso, o `doctor`
/// trava. E o pid 0 (`[System Process]`) é o topo: ali se para, não se sobe.
pub fn parent_chain(pid: u32) -> Vec<Entry> {
    let all = processes();
    let by_pid: HashMap<u32, &Entry> = all.iter().map(|entry| (entry.pid, entry)).collect();
    let mut chain = Vec::new();
    let mut seen = HashSet::from([pid]);
    let Some(start) = by_pid.get(&pid) else {
        return chain;
    };
    let mut current = start.parent;
    while chain.len() < MAX_DEPTH && current != 0 && seen.insert(current) {
        let Some(entry) = by_pid.get(&current) else {
            break;
        };
        chain.push((*entry).clone());
        current = entry.parent;
    }
    chain
}

/// O primeiro ancestral que é um shell — em que terminal este processo nasceu.
pub fn parent_shell() -> Option<ParentShell> {
    parent_chain(std::process::id())
        .iter()
        .find_map(|entry| classify(&entry.exe))
}

/// O nome do executável → o shell que ele é. Sem caixa: o Toolhelp devolve o
/// nome como o disco o escreveu, e `CMD.EXE` acontece.
pub fn classify(exe: &str) -> Option<ParentShell> {
    match exe.to_ascii_lowercase().as_str() {
        "cmd.exe" => Some(ParentShell::Cmd),
        "powershell.exe" => Some(ParentShell::WindowsPowerShell),
        "pwsh.exe" => Some(ParentShell::PowerShell7),
        "bash.exe" | "sh.exe" => Some(ParentShell::Bash),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O nome vem do disco, e a caixa dele não é promessa de ninguém.
    #[test]
    fn the_executable_name_is_matched_without_case() {
        assert_eq!(classify("cmd.exe"), Some(ParentShell::Cmd));
        assert_eq!(classify("CMD.EXE"), Some(ParentShell::Cmd));
        assert_eq!(
            classify("PowerShell.exe"),
            Some(ParentShell::WindowsPowerShell)
        );
        assert_eq!(classify("pwsh.exe"), Some(ParentShell::PowerShell7));
        assert_eq!(classify("bash.exe"), Some(ParentShell::Bash));
        assert_eq!(classify("sh.exe"), Some(ParentShell::Bash));
    }

    /// O que não é shell não vira shell — inclusive o próprio Claude Code, que
    /// é filho do shell e apareceria na cadeia antes dele.
    #[test]
    fn what_is_not_a_shell_is_not_classified() {
        assert_eq!(classify("claude.exe"), None);
        assert_eq!(classify("WindowsTerminal.exe"), None);
        assert_eq!(classify("explorer.exe"), None);
        assert_eq!(classify(""), None);
    }
}
