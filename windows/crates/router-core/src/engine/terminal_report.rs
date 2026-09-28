//! O estado da integração de terminal, shell por shell — o que a tela de Grupos
//! mostra e o `router doctor` confere.
//!
//! Existe porque os modos de falha daqui são SILENCIOSOS. Uma política de
//! execução `Restricted` (o padrão do Windows PowerShell 5.1 nas edições
//! cliente), um perfil que não carrega o script, ou um `.bash_profile` que
//! ignora o `.bashrc` não dão erro nenhum: `claude trabalho` simplesmente abre
//! no `~\.claude`, na conta errada. A tela só pode avisar do que sabe medir.
//!
//! Saiu do `doctor` na fase 5 (22/09/2026), com as mesmas regras, para o app e a
//! CLI contarem a mesma história. A consulta da política abre um PowerShell (não
//! é de graça), então quem monta o quadro a injeta — e só a faz onde a linha
//! está no perfil: sem ela, a política não decide nada.
//!
//! O Prompt de Comando entrou em 25/09/2026, e é a linha mais silenciosa de
//! todas: sem ele no quadro, `claude <grupo>` no cmd subia no perfil PADRÃO e
//! queimava a cota do grupo errado sem uma palavra. Ele está sempre presente e
//! não se instala junto com os outros — ver `fully_installed`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use regex::Regex;

use super::shell_integration::{ShellIntegration, ShellTargets};
use crate::platform::atomic_write::read_retrying;
use crate::platform::command_processor::{self, CommandProcessorKey};
use crate::platform::process::run_with_timeout;

/// O que identifica o nosso segmento no `AutoRun` — o mesmo marcador que o
/// `shell_integration` planta nos scripts. Repetido aqui porque lá ele é
/// privado ao módulo, e aquele arquivo é de outra trilha; divergindo os dois,
/// a tela diria "não instalada" com o `AutoRun` no lugar.
const SHIM_MARKER: &str = "falcao-router-shim";

/// Os shells em que `claude <grupo>` pode rodar.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ShellKind {
    /// O Windows PowerShell 5.1, que vem com o sistema.
    WindowsPowerShell,
    /// O PowerShell 7 (`pwsh`).
    PowerShell7,
    GitBash,
    /// O Prompt de Comando. Diferente dos outros três em duas coisas que o
    /// modelo precisa acomodar: está SEMPRE presente (não há máquina Windows sem
    /// ele, então a linha nunca é omitida por ausência), e não tem `$PROFILE` —
    /// quem carrega a integração é o `AutoRun` do registro.
    Cmd,
}

/// Uma edição do PowerShell instalada nesta máquina.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PowerShellEdition {
    pub kind: ShellKind,
    pub exe: PathBuf,
    /// A pasta do `$PROFILE` sob a Documentos.
    pub profile_dir: &'static str,
    /// A política quando nenhum escopo define uma.
    pub default_policy: &'static str,
}

/// Onde procurar as edições. Injetável: os testes montam uma máquina de mentira.
#[derive(Clone, Debug, Default)]
pub struct EditionEnv {
    pub system_root: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
    pub path: Option<OsString>,
}

impl EditionEnv {
    pub fn from_process() -> Self {
        EditionEnv {
            system_root: std::env::var_os("SystemRoot").map(PathBuf::from),
            program_files: std::env::var_os("ProgramFiles").map(PathBuf::from),
            path: std::env::var_os("PATH"),
        }
    }
}

/// As edições do PowerShell presentes: o 5.1 do sistema e o `pwsh` — no
/// `ProgramFiles` (instalador MSI) ou no PATH (o da Microsoft Store mora sob o
/// alias do WindowsApps, e era o caso da máquina do spike).
pub fn powershell_editions(env: &EditionEnv) -> Vec<PowerShellEdition> {
    let mut editions = Vec::new();
    if let Some(ps51) = env
        .system_root
        .as_ref()
        .map(|w| w.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|p| p.is_file())
    {
        editions.push(PowerShellEdition {
            kind: ShellKind::WindowsPowerShell,
            exe: ps51,
            profile_dir: "WindowsPowerShell",
            default_policy: "Restricted",
        });
    }
    let pwsh = env
        .program_files
        .as_ref()
        .map(|p| p.join(r"PowerShell\7\pwsh.exe"))
        .filter(|p| p.is_file())
        .or_else(|| {
            env.path.as_ref().and_then(|path| {
                std::env::split_paths(path)
                    .map(|d| d.join("pwsh.exe"))
                    .find(|p| p.is_file())
            })
        });
    if let Some(pwsh) = pwsh {
        editions.push(PowerShellEdition {
            kind: ShellKind::PowerShell7,
            exe: pwsh,
            profile_dir: "PowerShell",
            default_policy: "RemoteSigned",
        });
    }
    editions
}

/// A política efetiva a partir da saída de `Get-ExecutionPolicy -List`
/// (`Escopo=Política` por linha), IGNORANDO o escopo Process: o shell em que o
/// app ou o `doctor` rodam pode ter herdado `Bypass` (o do Claude Code herda), e
/// o que decide se o `$PROFILE` roda num terminal novo são os outros escopos,
/// na ordem de precedência.
pub fn parse_policy_list(output: &str, default: &str) -> String {
    let scopes: HashMap<&str, &str> = output
        .lines()
        .filter_map(|l| l.trim().split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    ["MachinePolicy", "UserPolicy", "CurrentUser", "LocalMachine"]
        .iter()
        .filter_map(|scope| scopes.get(scope))
        .find(|p| **p != "Undefined")
        .map_or_else(|| default.to_string(), |p| p.to_string())
}

/// Um PowerShell filho SEM o ambiente do processo que o abriu.
///
/// Medido em 25/09/2026: um 5.1 filho de um pwsh 7 herda o `PSModulePath` do 7,
/// não acha o `Microsoft.PowerShell.Security` dele e fica sem
/// `Get-ExecutionPolicy`/`Set-ExecutionPolicy` — o `doctor` imprimia "não foi
/// possível consultar a política" bem onde a resposta importa. O
/// `PSExecutionPolicyPreference` sai junto: é o escopo Process herdado (o shell
/// do Claude Code roda com `Bypass`), e ele não decide nada num terminal novo.
fn clean_powershell(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    command.env_remove("PSModulePath");
    command.env_remove("PSExecutionPolicyPreference");
    command.args(["-NoProfile", "-NonInteractive", "-Command"]);
    command
}

/// Consulta a política efetiva de uma edição (abre um PowerShell; até 30 s).
pub fn effective_policy(edition: &PowerShellEdition) -> Option<String> {
    let mut command = clean_powershell(&edition.exe);
    command.arg(
        "Get-ExecutionPolicy -List | ForEach-Object { '{0}={1}' -f $_.Scope, $_.ExecutionPolicy }",
    );
    let (code, out) = run_with_timeout(command, None, Duration::from_secs(30))?;
    (code == Some(0)).then(|| parse_policy_list(&out, edition.default_policy))
}

/// Com estas, o `$PROFILE` não roda: o PowerShell recusa o script não assinado.
pub fn policy_blocks_profiles(policy: &str) -> bool {
    matches!(policy, "Restricted" | "AllSigned")
}

/// A correção consentida: `RemoteSigned` no escopo do usuário (sem admin), na
/// edição que estava bloqueando. Diretiva de grupo (MachinePolicy/UserPolicy)
/// continua vencendo — por isso quem chama confere de novo depois.
pub fn allow_profiles(edition: &PowerShellEdition) -> bool {
    // Mesmo ambiente limpo da consulta: o `Set-ExecutionPolicy` vem do mesmo
    // módulo que some quando o `PSModulePath` é herdado de outra edição.
    let mut command = clean_powershell(&edition.exe);
    command.arg("Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force");
    matches!(
        run_with_timeout(command, None, Duration::from_secs(30)),
        Some((Some(0), _))
    )
}

/// O texto de um perfil do PowerShell, no UTF-16LE com BOM que o 5.1 usa ao
/// salvar ou em UTF-8/ANSI.
fn profile_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// O perfil já define uma função `claude`? A integração a encadeia (é ela que
/// continua atendendo o `claude` sem grupo) — a tela diz isso em vez de deixar
/// o usuário achar que perdeu a dele.
pub fn defines_claude_function(profile: &Path) -> bool {
    let Ok(bytes) = read_retrying(profile) else {
        return false;
    };
    // Depois do nome vem espaço, `{`, `(` ou o fim da linha — `\b` sozinho
    // casaria `function claude-gov` (o `-` é fronteira de palavra), e era o que
    // o `doctor` fazia até a fase 5.
    Regex::new(r"(?im)^\s*function\s+(?:global:)?claude(?:\s|\{|\(|$)")
        .map(|re| re.is_match(&profile_text(&bytes)))
        .unwrap_or(false)
}

/// Quem o Git Bash lê ao abrir: o primeiro perfil de login que existir.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BashLogin {
    /// Nenhum dos três: o Git Bash cria um `.bash_profile` com um WARNING
    /// vermelho (ativar a integração cria antes o mesmo arquivo).
    Missing,
    /// Carrega o `.bashrc` — a integração roda.
    Loads(PathBuf),
    /// Existe e não carrega o `.bashrc`: a linha do router nunca roda.
    Ignores(PathBuf),
}

pub fn bash_login_profile(home: &Path) -> BashLogin {
    let Some(first) = [".bash_profile", ".bash_login", ".profile"]
        .iter()
        .map(|n| home.join(n))
        .find(|p| p.exists())
    else {
        return BashLogin::Missing;
    };
    let loads =
        read_retrying(&first).is_ok_and(|b| String::from_utf8_lossy(&b).contains(".bashrc"));
    if loads {
        BashLogin::Loads(first)
    } else {
        BashLogin::Ignores(first)
    }
}

/// Os scripts `claude` do router.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptsState {
    /// Nunca instalados (ou apagados).
    Missing,
    /// Citam este `router.exe`.
    Current,
    /// Citam OUTRO binário: o app foi movido ou reinstalado noutro lugar.
    Stale,
}

/// O `shell.ps1` (e o `shell.sh`, havendo Git Bash) citam ESTE binário?
pub fn scripts_state(ps1: &Path, sh: &Path, router: &Path, with_bash: bool) -> ScriptsState {
    let mut scripts = vec![(ps1, router.to_string_lossy().replace('\'', "''"))];
    if with_bash {
        scripts.push((sh, router.to_string_lossy().replace('\\', "/")));
    }
    let mut state = ScriptsState::Current;
    for (path, needle) in scripts {
        match read_retrying(path) {
            Err(_) => return ScriptsState::Missing,
            Ok(bytes) if String::from_utf8_lossy(&bytes).contains(needle.as_str()) => {}
            Ok(_) => state = ScriptsState::Stale,
        }
    }
    state
}

/// Um shell presente nesta máquina e o que se sabe dele.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShellReport {
    pub kind: ShellKind,
    /// Os arquivos que a integração edita neste shell (`$PROFILE` de cada host,
    /// ou `~\.bashrc`). É uma LISTA porque uma edição do PowerShell tem um
    /// perfil por host — o console e o Console Integrado do VS Code leem
    /// arquivos diferentes, e cobrir só o primeiro deixava o segundo sem
    /// integração, em silêncio. No `Cmd` é o caminho do `shell.cmd`.
    pub profiles: Vec<PathBuf>,
    /// O perfil carrega o script do router.
    pub loads_integration: bool,
    /// A política efetiva (só PowerShell, e só consultada com a linha no lugar).
    pub policy: Option<String>,
    /// A política impede o perfil de rodar: `claude <grupo>` cai no `claude` puro.
    pub policy_blocks: bool,
    /// O perfil já tinha uma função `claude`, que a integração encadeia.
    pub chains_user_function: bool,
    /// Git Bash: quem carrega o `.bashrc` ao abrir (`None` nos PowerShell).
    pub bash_login: Option<BashLogin>,
}

/// O quadro inteiro da integração de terminal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TerminalReport {
    pub scripts: ScriptsState,
    /// Na ordem PowerShell 7, Windows PowerShell 5.1, Git Bash (só os
    /// presentes) e o Prompt de Comando, que está sempre lá.
    pub shells: Vec<ShellReport>,
    /// Com o Developer Mode, `CLAUDE.md`, `keybindings.json` e o histórico são
    /// links; sem ele, cópias sincronizadas e histórico por grupo.
    pub developer_mode: bool,
}

impl TerminalReport {
    /// Monta o quadro. `policy_of` consulta a política de uma edição — só é
    /// chamada onde a linha está no perfil.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        targets: &ShellTargets,
        ps1: &Path,
        sh: &Path,
        router: &Path,
        editions: &[PowerShellEdition],
        git_bash: Option<&Path>,
        developer_mode: bool,
        mut policy_of: impl FnMut(&PowerShellEdition) -> Option<String>,
    ) -> TerminalReport {
        let ps_line = ShellIntegration::powershell_source_line(ps1);
        let mut shells = Vec::new();
        for kind in [ShellKind::PowerShell7, ShellKind::WindowsPowerShell] {
            let Some(edition) = editions.iter().find(|e| e.kind == kind) else {
                continue; // edição ausente não tem terminal para quebrar
            };
            let Some(profile) = targets.powershell_profiles.iter().find(|p| {
                p.parent()
                    .and_then(Path::file_name)
                    .is_some_and(|f| f.eq_ignore_ascii_case(edition.profile_dir))
            }) else {
                continue; // sem a Documentos não há `$PROFILE` a editar
            };
            let loads = ShellIntegration::profile_has_line(profile, &ps_line);
            let policy = if loads { policy_of(edition) } else { None };
            shells.push(ShellReport {
                kind,
                profiles: vec![profile.clone()],
                loads_integration: loads,
                policy_blocks: policy.as_deref().is_some_and(policy_blocks_profiles),
                policy,
                chains_user_function: loads && defines_claude_function(profile),
                bash_login: None,
            });
        }
        if git_bash.is_some() {
            let line = ShellIntegration::bash_source_line(sh);
            shells.push(ShellReport {
                kind: ShellKind::GitBash,
                profiles: vec![targets.bashrc.clone()],
                loads_integration: ShellIntegration::profile_has_line(&targets.bashrc, &line),
                policy: None,
                policy_blocks: false,
                chains_user_function: false,
                bash_login: Some(bash_login_profile(&targets.home)),
            });
        }
        // O cmd nunca é omitido por ausência: não existe Windows sem ele. O que
        // varia é só o `AutoRun` já carregar o `shell.cmd` — e nada de política,
        // encadeamento ou perfil de login se aplica aqui.
        shells.push(ShellReport {
            kind: ShellKind::Cmd,
            // O `shell.cmd` é irmão do `shell.ps1` (os três scripts saem da
            // mesma pasta de dados), então o quadro o deriva em vez de pedir
            // mais um parâmetro a quem chama.
            profiles: vec![ps1.with_file_name("shell.cmd")],
            loads_integration: command_processor::has_marker(
                &CommandProcessorKey::current_user(),
                SHIM_MARKER,
            ),
            policy: None,
            policy_blocks: false,
            chains_user_function: false,
            bash_login: None,
        });
        TerminalReport {
            scripts: scripts_state(ps1, sh, router, git_bash.is_some()),
            shells,
            developer_mode,
        }
    }

    pub fn shell(&self, kind: ShellKind) -> Option<&ShellReport> {
        self.shells.iter().find(|s| s.kind == kind)
    }

    /// Scripts atuais e todo shell presente carregando a integração, sem nada
    /// que a impeça de rodar.
    ///
    /// O cmd NÃO entra na conta: mexer no `AutoRun` é escrever um valor GLOBAL
    /// do usuário — ele roda em toda invocação de `cmd.exe`, inclusive as de
    /// terceiros —, então é opt-in por botão próprio, como o "Permitir" da
    /// política. Quem não o escolheu não pode ficar com a tela vermelha para
    /// sempre.
    pub fn fully_installed(&self) -> bool {
        let required = || self.shells.iter().filter(|s| s.kind != ShellKind::Cmd);
        // Lista vazia faz o `all` dizer `true`, e ela fica vazia justamente
        // quando a Documentos não é achada: sem esta exigência a tela dizia
        // "Instalada" com ZERO shell coberto — o modo de falha silencioso que
        // este arquivo existe para combater.
        self.scripts == ScriptsState::Current
            && required().next().is_some()
            && required().all(|s| {
                s.loads_integration
                    && !s.policy_blocks
                    && !matches!(s.bash_login, Some(BashLogin::Ignores(_)))
            })
    }

    /// Algum PowerShell com a linha no perfil e a política recusando o script.
    pub fn blocked_by_policy(&self) -> bool {
        self.shells.iter().any(|s| s.policy_blocks)
    }
}
