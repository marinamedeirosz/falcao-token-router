//! A função de shell: os scripts, os dois `$PROFILE`, o `.bashrc` e a política de execução.
//!
//! Checagens do `router doctor` — a ordem está no `mod.rs` ao lado.

use super::*;

/// Os scripts existem e citam ESTE binário?
pub(super) fn check_scripts(
    report: &mut Report,
    ps1: &Path,
    sh: &Path,
    me: &Path,
    with_bash: bool,
) {
    let scripts: Vec<(&Path, &str, String)> = {
        let mut v = vec![(ps1, "shell.ps1", me.to_string_lossy().replace('\'', "''"))];
        if with_bash {
            v.push((sh, "shell.sh", me.to_string_lossy().replace('\\', "/")));
        }
        v
    };
    for (path, name, needle) in scripts {
        match read_retrying(path) {
            Err(_) => report.check(
                false,
                format!("{name} ausente — Grupos → Integração com o terminal → Ativar"),
            ),
            Ok(bytes) if String::from_utf8_lossy(&bytes).contains(needle.as_str()) => {
                report.check(true, format!("{name} aponta para este binário"))
            }
            Ok(_) => report.check(
                false,
                format!(
                    "{name} aponta para OUTRO binário (app movido ou reinstalado) — abra o app para reparar"
                ),
            ),
        }
    }
}

/// A linha nos dois `$PROFILE` (e no `.bashrc`), a política de execução de cada
/// edição, e uma função `claude` do usuário que a integração encadeia.
pub(super) fn check_profiles(
    report: &mut Report,
    targets: &ShellTargets,
    ps1: &Path,
    sh: &Path,
    with_bash: bool,
) {
    let ps_line = ShellIntegration::powershell_source_line(ps1);
    let editions = powershell_editions(&EditionEnv::from_process());
    for profile in &targets.powershell_profiles {
        let folder = profile
            .parent()
            .and_then(Path::file_name)
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let label = if folder.eq_ignore_ascii_case("WindowsPowerShell") {
            "Windows PowerShell 5.1"
        } else {
            "PowerShell 7"
        };
        let Some(edition) = editions
            .iter()
            .find(|e| folder.eq_ignore_ascii_case(e.profile_dir))
        else {
            // Uma edição que não existe aqui não tem terminal para quebrar.
            report.info(format!(
                "{label} não encontrado nesta máquina — o $PROFILE dele não é conferido"
            ));
            continue;
        };
        let has = ShellIntegration::profile_has_line(profile, &ps_line);
        if has {
            report.check(true, format!("$PROFILE do {label} carrega o shell.ps1"));
            if defines_claude_function(profile) {
                report.info(format!(
                    "o $PROFILE do {label} define uma função `claude`: a integração a encadeia — `claude` sem grupo continua passando por ela"
                ));
            }
        } else {
            report.check(
                false,
                format!(
                    "$PROFILE do {label} não carrega o shell.ps1 ({}) — Grupos → Integração com o terminal → Ativar",
                    profile.display()
                ),
            );
        }
        // A política só importa onde a integração deveria rodar (e consultá-la
        // abre um PowerShell, que não é de graça).
        if has {
            match effective_policy(edition) {
                Some(policy) if policy_blocks_profiles(&policy) => report.check(
                    false,
                    format!(
                        "política de execução do {label}: {policy} — o $PROFILE não roda e `claude <grupo>` cai no claude puro, na conta errada. Corrija: Set-ExecutionPolicy -Scope CurrentUser RemoteSigned"
                    ),
                ),
                Some(policy) => {
                    report.check(true, format!("política de execução do {label}: {policy}"))
                }
                None => report.info(format!(
                    "não foi possível consultar a política de execução do {label}"
                )),
            }
        }
    }
    if with_bash {
        let sh_line = ShellIntegration::bash_source_line(sh);
        let has = ShellIntegration::profile_has_line(&targets.bashrc, &sh_line);
        report.check(
            has,
            if has {
                "~/.bashrc carrega o shell.sh (Git Bash)".to_string()
            } else {
                "~/.bashrc não carrega o shell.sh (Git Bash) — Grupos → Integração com o terminal → Ativar".to_string()
            },
        );
        check_bash_profile(report, &targets.home);
    }
}

/// O Git Bash só lê o `.bashrc` se um perfil de login o carregar.
pub(super) fn check_bash_profile(report: &mut Report, home: &Path) {
    match bash_login_profile(home) {
        BashLogin::Missing => report.check(
            false,
            "sem ~/.bash_profile — o Git Bash não carrega o ~/.bashrc (e avisa em vermelho). Ativar a integração no app o cria",
        ),
        BashLogin::Loads(first) => {
            report.check(true, format!("{} carrega o ~/.bashrc", first.display()))
        }
        BashLogin::Ignores(first) => report.check(
            false,
            format!(
                "{} não carrega o ~/.bashrc — acrescente: test -f ~/.bashrc && . ~/.bashrc",
                first.display()
            ),
        ),
    }
}

// --- o Prompt de Comando, e o shell em que o usuário está AGORA ---

/// O `AutoRun` do Prompt de Comando, do ponto de vista de quem diagnostica.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CmdAutorun {
    /// Sem o nosso `shell.cmd` no valor: ausente de vez, ou só com o comando de
    /// terceiro (clink, ConEmu, Anaconda) que já morava ali.
    Off,
    /// Carrega o nosso `shell.cmd` — `claude <grupo>` funciona no cmd.
    Ours,
    /// Carrega um `shell.cmd` que não é este: o app mudou de lugar (ou foi
    /// reinstalado noutro), e o `AutoRun` ficou apontando para o caminho velho.
    Elsewhere,
}

/// Lê o `AutoRun` e o compara com o `shell.cmd` que ESTA instalação espera.
///
/// O marcador é o CAMINHO do script, e não uma palavra nossa: é ele que o valor
/// tem de citar para o `cmd.exe` carregar a macro, e assim a checagem não
/// depende de como a instalação compôs a linha (aspas, `call`, o separador de
/// quem já estava lá).
pub(super) fn cmd_autorun(cmd_script: &Path) -> CmdAutorun {
    let key = CommandProcessorKey::current_user();
    if has_marker(&key, &cmd_script.to_string_lossy()) {
        return CmdAutorun::Ours;
    }
    match autorun(&key) {
        Some(value) if value.to_lowercase().contains("shell.cmd") => CmdAutorun::Elsewhere,
        _ => CmdAutorun::Off,
    }
}

/// Em que shell o usuário está AGORA — a checagem que faltava.
///
/// Todas as outras conferem o DISCO: os scripts, os perfis, a política. Nenhuma
/// perguntava em que shell `claude <grupo>` seria digitado, e era por isso que o
/// `doctor` dizia "tudo certo" para quem estava no Prompt de Comando sem a
/// integração dele — o único caso que ele não enxergava era o pior de todos: a
/// sessão sobe no perfil PADRÃO e queima a cota do grupo errado, em silêncio.
///
/// O quadro (`TerminalReport`) já sabe o estado de cada shell; aqui só se
/// escolhe a linha do shell em que o `doctor` foi rodado.
pub(super) fn check_current_shell(
    report: &mut Report,
    targets: &ShellTargets,
    ps1: &Path,
    sh: &Path,
    me: &Path,
    git_bash: Option<&Path>,
    cmd: CmdAutorun,
) {
    let Some(shell) = parent_shell() else {
        // Não saber não é o mesmo que estar errado: a cadeia de processos pode
        // ter sido cortada (um lançador, um terminal que reabre o shell).
        report.info(
            "shell atual: não descobri em que shell você está — as checagens acima valem para todos",
        );
        return;
    };
    let name = shell_name(shell);

    if shell == ParentShell::Cmd {
        // O cmd não tem `$PROFILE`: quem decide aqui é o `AutoRun`.
        let ok = cmd == CmdAutorun::Ours;
        report.check(
            ok,
            if ok {
                format!("shell atual: {name}, com a integração ativa")
            } else {
                // A verdade, e não a desculpa de sempre ("terminal aberto antes
                // da integração"): no cmd abrir outro terminal não muda NADA —
                // sem `AutoRun` nenhum terminal dele carrega a integração, e
                // repetir a ação inútil é o que faz a pessoa desistir.
                format!(
                    "shell atual: {name}, SEM a integração — aqui `claude <grupo>` abre o claude puro, na conta errada, e a sessão queima a cota do grupo PADRÃO. Abrir outro terminal não muda nada: ative em Grupos → Integração com o terminal → Ativar no cmd"
                )
            },
        );
        return;
    }

    let editions = powershell_editions(&EditionEnv::from_process());
    let terminal = TerminalReport::build(
        targets,
        ps1,
        sh,
        me,
        &editions,
        git_bash,
        false,
        // A política só é consultada para a edição em que o usuário ESTÁ: cada
        // consulta abre um PowerShell, e o `check_profiles` acima já pagou a de
        // toda edição que tem a linha no perfil.
        |edition| {
            (shell_kind(shell) == edition.kind)
                .then(|| effective_policy(edition))
                .flatten()
        },
    );
    let Some(state) = terminal.shell(shell_kind(shell)) else {
        report.info(format!(
            "shell atual: {name} — não foi possível conferir a integração dele nesta máquina"
        ));
        return;
    };
    let loads = loads_integration(state);
    report.check(
        loads,
        if loads {
            format!("shell atual: {name}, que carrega a integração")
        } else {
            format!(
                "shell atual: {name}, e ele NÃO carrega a integração — aqui `claude <grupo>` abre o claude puro, na conta errada (o motivo está nas linhas acima)"
            )
        },
    );
}

/// O estado do `AutoRun`: a linha que diz se o Prompt de Comando está coberto.
///
/// Sem o nosso script NÃO é falha: a integração do cmd é opt-in por botão
/// próprio, porque o `AutoRun` é um valor GLOBAL do usuário — roda em toda
/// invocação de `cmd.exe`, inclusive as de terceiros. Quem não a escolheu não
/// pode ficar com um diagnóstico vermelho para sempre; e quem está NO cmd sem
/// ela já falhou na checagem do shell atual, logo acima.
pub(super) fn check_autorun(report: &mut Report, cmd_script: &Path, state: CmdAutorun) {
    match state {
        CmdAutorun::Ours => report.check(true, "AutoRun do Prompt de Comando carrega o shell.cmd"),
        CmdAutorun::Elsewhere => report.check(
            false,
            format!(
                "AutoRun do Prompt de Comando aponta para OUTRO shell.cmd (app movido ou reinstalado) — o esperado é {}. Reative em Grupos → Integração com o terminal → Ativar no cmd",
                cmd_script.display()
            ),
        ),
        CmdAutorun::Off => report.info(
            "AutoRun do Prompt de Comando: sem o nosso shell.cmd — a integração do cmd é opt-in (Grupos → Integração com o terminal → Ativar no cmd)",
        ),
    }
}

/// Neste shell, AGORA, `claude <grupo>` passa pelo router?
///
/// Não é a mesma pergunta que "a instalação está completa": as três formas de
/// não carregar são silenciosas, e cada uma já tem a sua linha acima — esta só
/// as junta para não dizer "tudo certo" logo depois de uma delas falhar.
fn loads_integration(state: &ShellReport) -> bool {
    state.loads_integration
        && !state.policy_blocks
        // `Missing` conta junto com `Ignores`: sem nenhum perfil de login, o Git
        // Bash NÃO leu o `.bashrc` nesta sessão — ele cria o `.bash_profile` e
        // avisa em vermelho, e quem carrega a integração é só a próxima.
        && !matches!(
            state.bash_login,
            Some(BashLogin::Ignores(_) | BashLogin::Missing)
        )
}

fn shell_name(shell: ParentShell) -> &'static str {
    match shell {
        ParentShell::Cmd => "Prompt de Comando",
        ParentShell::WindowsPowerShell => "Windows PowerShell 5.1",
        ParentShell::PowerShell7 => "PowerShell 7",
        ParentShell::Bash => "Git Bash",
    }
}

/// O shell da árvore de processos → o do quadro. O `process_tree` não conhece o
/// `engine` de propósito, então o mapeamento é de quem usa os dois.
fn shell_kind(shell: ParentShell) -> ShellKind {
    match shell {
        ParentShell::Cmd => ShellKind::Cmd,
        ParentShell::WindowsPowerShell => ShellKind::WindowsPowerShell,
        ParentShell::PowerShell7 => ShellKind::PowerShell7,
        ParentShell::Bash => ShellKind::GitBash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state(kind: ShellKind) -> ShellReport {
        ShellReport {
            kind,
            profiles: vec![PathBuf::from(r"C:\Users\exemplo\.bashrc")],
            loads_integration: true,
            policy: None,
            policy_blocks: false,
            chains_user_function: false,
            bash_login: None,
        }
    }

    /// Cada uma das quatro basta para `claude <grupo>` cair no claude puro —
    /// e o `doctor` dizia "tudo certo" em todas até esta trilha.
    #[test]
    fn a_shell_loads_the_integration_only_when_nothing_stops_the_profile() {
        assert!(loads_integration(&state(ShellKind::PowerShell7)));

        let mut without_line = state(ShellKind::PowerShell7);
        without_line.loads_integration = false;
        assert!(!loads_integration(&without_line));

        let mut blocked = state(ShellKind::WindowsPowerShell);
        blocked.policy = Some("Restricted".into());
        blocked.policy_blocks = true;
        assert!(!loads_integration(&blocked));

        let profile = PathBuf::from(r"C:\Users\exemplo\.bash_profile");
        for login in [
            BashLogin::Ignores(profile.clone()),
            BashLogin::Missing,
            BashLogin::Loads(profile),
        ] {
            let loads = matches!(login, BashLogin::Loads(_));
            let mut bash = state(ShellKind::GitBash);
            bash.bash_login = Some(login);
            assert_eq!(loads_integration(&bash), loads);
        }
    }
}
