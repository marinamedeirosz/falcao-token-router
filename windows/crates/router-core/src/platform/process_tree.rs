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
///
/// TRILHA T3: mover para cá o corpo de `job.rs::test_support::processes` e fazer
/// o `test_support` chamar esta.
pub fn processes() -> Vec<Entry> {
    Vec::new()
}

/// A cadeia de ancestrais de um pid, do pai para cima.
///
/// TRILHA T3: subir a árvore com um teto (12 basta — ninguém aninha terminal
/// mais que isso) E um conjunto de visitados. Os dois, não um: a foto do
/// Toolhelp não é atômica e um pid reciclado pode apontar para trás, fechando
/// ciclo. Sem parada garantida, isto trava o `doctor`.
pub fn parent_chain(_pid: u32) -> Vec<Entry> {
    Vec::new()
}

/// O primeiro ancestral que é um shell — em que terminal este processo nasceu.
///
/// TRILHA T3: `parent_chain(std::process::id())` e o primeiro `exe` que casar
/// com `classify`.
pub fn parent_shell() -> Option<ParentShell> {
    None
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
