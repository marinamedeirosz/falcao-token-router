//! Integração com o terminal: os scripts de shell e a status line.
//!
//! Parte do `impl RouterConfigStore` — ver o `mod.rs` ao lado.

use super::*;
use crate::engine::shell_integration::{autorun_call, SHIM_MARKER};
use crate::platform::command_processor::{self, CommandProcessorKey};

impl RouterConfigStore {
    // MARK: - Integração com o terminal

    pub fn set_router_path(&mut self, path: Option<PathBuf>) {
        self.router_path = path;
    }

    pub fn router_path(&self) -> Option<&Path> {
        self.router_path.as_deref()
    }

    /// O `shell.ps1` que os perfis do PowerShell carregam.
    pub fn powershell_script_path(&self) -> PathBuf {
        self.paths.base.join("shell.ps1")
    }

    /// O `shell.sh` que o `~\.bashrc` do Git Bash carrega.
    pub fn bash_script_path(&self) -> PathBuf {
        self.paths.base.join("shell.sh")
    }

    /// O `shell.cmd` que o `AutoRun` do Prompt de Comando carrega.
    ///
    /// Mora na pasta de DADOS, não na de instalação, e é de propósito: o
    /// desinstalador não apaga esta pasta, então o script sobrevive e a guarda
    /// `if not exist` dentro dele mantém o terminal limpo quando o `router.exe`
    /// some.
    pub fn cmd_script_path(&self) -> PathBuf {
        self.paths.base.join("shell.cmd")
    }

    /// A status line que o perfil de um grupo deve ter, para o shell que o
    /// Claude Code vai usar.
    pub fn expected_status_line(&self, group: &AccountGroup, shell: StatusShell) -> Option<String> {
        let router = self.router_path.as_deref()?;
        Some(ShellIntegration::status_line_for_profile(
            router,
            &group.config_dir,
            shell,
        ))
    }

    /// Escreve os scripts, planta a status line e o compartilhamento em cada
    /// perfil de grupo, e acrescenta a linha nos perfis de shell. Idempotente.
    /// A parte do Git Bash só entra quando há Git Bash (`shell == Bash`).
    pub fn install_shell_integration(
        &mut self,
        targets: &ShellTargets,
        shell: StatusShell,
    ) -> Result<(), StoreError> {
        let Some(router) = self.router_path.clone() else {
            self.last_error = Some(StoreError::RouterPathUnknown);
            return Err(StoreError::RouterPathUnknown);
        };
        let (ps1, sh, cmd) = (
            self.powershell_script_path(),
            self.bash_script_path(),
            self.cmd_script_path(),
        );
        let mut failures: Vec<String> = Vec::new();

        if let Err(e) = ShellIntegration::write_scripts(&router, &ps1, &sh, &cmd) {
            failures.push(e.to_string());
        }
        let home = ConfigDir::standard(&self.home);
        for group in &self.config.groups {
            if let Some(command) = self.expected_status_line(group, shell) {
                if let Err(e) = ShellIntegration::install_status_line(&command, &group.config_dir) {
                    failures.push(e.to_string());
                }
            }
            ProfileSharing::link(
                &group.config_dir,
                &home,
                self.config.share_history,
                &self.paths.base,
            );
        }
        let ps_line = ShellIntegration::powershell_source_line(&ps1);
        for profile in &targets.powershell_profiles {
            if let Err(e) = ShellIntegration::ensure_in_profile(profile, &ps_line, "\r\n") {
                failures.push(format!("{}: {e}", profile.display()));
            }
        }
        if shell == StatusShell::Bash {
            let sh_line = ShellIntegration::bash_source_line(&sh);
            if let Err(e) = ShellIntegration::ensure_in_profile(&targets.bashrc, &sh_line, "\n") {
                failures.push(format!("{}: {e}", targets.bashrc.display()));
            }
            if let Err(e) = ShellIntegration::ensure_bash_profile(targets) {
                failures.push(e.to_string());
            }
        }

        if failures.is_empty() {
            self.last_error = None;
            Ok(())
        } else {
            let error = StoreError::IntegrationFailed(failures.join("; "));
            self.last_error = Some(error.clone());
            Err(error)
        }
    }

    /// Liga a integração do Prompt de Comando: o `AutoRun` do usuário passa a
    /// carregar o `shell.cmd`, e aí `claude <grupo>` funciona no cmd.
    ///
    /// Fora do `install_shell_integration` de propósito. O `AutoRun` é um valor
    /// GLOBAL do usuário — clink, ConEmu e Anaconda escrevem no mesmo lugar — e
    /// roda em TODA invocação de `cmd.exe`, inclusive as que npm, MSBuild e as
    /// tarefas do VS Code disparam. Mexer nele é escolha explícita, com botão e
    /// confirmação próprios; é por isso que `fully_installed` e `needs_install`
    /// já ignoram a linha do cmd. O `shell.cmd` em si o "Ativar" geral já
    /// gravou (`write_scripts`), então o alvo existe antes de o AutoRun apontar.
    pub fn enable_cmd_integration(&mut self) -> Result<(), StoreError> {
        self.enable_cmd_integration_with(&CommandProcessorKey::current_user())
    }

    /// Idem, com a chave injetada — os testes escrevem sob uma subchave própria.
    pub fn enable_cmd_integration_with(
        &mut self,
        key: &CommandProcessorKey,
    ) -> Result<(), StoreError> {
        // O caminho vai entre aspas (a pasta de dados pode ter espaço) e o
        // marcador vai JUNTO, como argumento que o `shell.cmd` ignora: é ele
        // que identifica o nosso segmento na hora de tirar, e é ele que a tela
        // procura para dizer "instalada". O caminho sozinho não serviria — a
        // pasta de dados se chama `com.synqo.falcao-router`, que NÃO contém o
        // marcador, e sem reconhecê-lo cada clique somaria um segmento igual.
        let call = autorun_call(&self.cmd_script_path());
        let done = command_processor::install(key, &call, SHIM_MARKER).map(|_| ());
        self.record_integration(done)
    }

    /// Desliga: tira SÓ o nosso segmento do `AutoRun`, preservando o de quem
    /// mais estiver lá.
    pub fn disable_cmd_integration(&mut self) -> Result<(), StoreError> {
        self.disable_cmd_integration_with(&CommandProcessorKey::current_user())
    }

    /// Idem, com a chave injetada.
    pub fn disable_cmd_integration_with(
        &mut self,
        key: &CommandProcessorKey,
    ) -> Result<(), StoreError> {
        let done = command_processor::remove(key, SHIM_MARKER).map(|_| ());
        self.record_integration(done)
    }

    /// Guarda a falha para a UI mostrar, como as outras ações do store fazem.
    fn record_integration(&mut self, done: io::Result<()>) -> Result<(), StoreError> {
        done.map_err(|e| {
            let error = StoreError::IntegrationFailed(e.to_string());
            self.last_error = Some(error.clone());
            error
        })
    }

    /// A integração instalada aponta para um binário que não é mais este (app
    /// movido ou reinstalado noutro lugar), ou algum perfil de grupo está sem a
    /// status line certa, ou algum dos três scripts é de outra versão. O modo
    /// de falha é silencioso — `claude trabalho` cai no `claude` puro, na conta
    /// errada — por isso a cura é automática.
    /// Não instalada não é obsoleta: é ausente.
    pub fn integration_is_stale(&self, shell: StatusShell) -> bool {
        let Some(router) = self.router_path.as_deref() else {
            return false;
        };
        let ps1 = self.powershell_script_path();
        // Ausente não é obsoleto, e o `shell.ps1` é a âncora dessa pergunta:
        // sem ele nunca houve instalação, e "curar" seria instalar no lugar de
        // quem não pediu (a cura roda sozinha na subida do app).
        if read_retrying(&ps1).is_err() {
            return false;
        }
        let expected = [
            (ps1, ShellIntegration::powershell_script(router)),
            (
                self.bash_script_path(),
                ShellIntegration::bash_script(router),
            ),
            (self.cmd_script_path(), ShellIntegration::cmd_script(router)),
        ];
        if expected
            .iter()
            .any(|(path, text)| !script_is_current(path, text))
        {
            return true;
        }
        self.config.groups.iter().any(|group| {
            self.expected_status_line(group, shell)
                .is_some_and(|cmd| ShellIntegration::status_line_is_stale(&cmd, &group.config_dir))
        })
    }

    /// Reinstala quando ficou obsoleta. Devolve `true` se mexeu em algo.
    pub fn heal_shell_integration(&mut self, targets: &ShellTargets, shell: StatusShell) -> bool {
        self.integration_is_stale(shell) && self.install_shell_integration(targets, shell).is_ok()
    }
}

/// O script no disco é o que ESTA versão geraria?
///
/// Comparar conteúdo, e não "o script cita o caminho do router", é o que faz um
/// conserto no shim chegar a quem já instalou: a pasta de instalação não muda
/// entre versões, então o script ANTIGO cita o mesmo caminho e passaria por
/// atual para sempre — a cura na subida não fazia nada e a tela dizia
/// "Instalada" com o script velho no disco. É exato, dispensa numerar versão, e
/// é o mesmo padrão do `install_status_line`, que compara o valor inteiro.
fn script_is_current(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = read_retrying(path) else {
        // Conteúdo vazio não vira arquivo (ver `write_scripts`): aí o ausente é
        // o estado certo, e não um script obsoleto.
        return expected.is_empty();
    };
    normalize(&String::from_utf8_lossy(&bytes)) == normalize(expected)
}

/// Sem BOM e com LF. O `shell.ps1` é gravado em UTF-8 COM BOM e CRLF e o
/// `shell.cmd` em CRLF, enquanto o gerador devolve LF puro: comparar cru
/// acusaria diferença que só existe na codificação, e a integração seria
/// reinstalada a cada subida do app.
fn normalize(text: &str) -> String {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
}
