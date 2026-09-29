//! `router uninstall-integration` — desfaz tudo o que a integração escreveu.
//!
//! Até aqui o produto só sabia INSTALAR. A remoção era um passo manual no
//! README ("abra o `notepad $PROFILE` e apague a linha"), com três consequências
//! ruins: quem desinstalava o app ficava com uma função `claude` quebrada
//! avisando em vermelho para sempre; não havia como o app oferecer "Desativar";
//! e nenhuma correção podia MOVER a linha de lugar, só acrescentar.
//!
//! Três donos chamam isto: o botão "Desativar" da tela Grupos, o
//! `NSIS_HOOK_PREUNINSTALL` (que roda com o `router.exe` ainda no lugar) e o
//! usuário pela linha de comando.
//!
//! **Best-effort e falante:** cada peça é tentada mesmo que a anterior tenha
//! falhado — um `$PROFILE` preso por outro processo não pode deixar o `AutoRun`
//! para trás, que é o pedaço que roda em TODA invocação do `cmd.exe`. O que
//! falhou é nomeado na saída, e só então o código de saída é 1. O que não estava
//! lá nunca é erro: desinstalar duas vezes tem de dar certo as duas.
//!
//! **O que NÃO sai:** os scripts (`shell.ps1`, `shell.sh`, `shell.cmd`) e a
//! pasta de dados. Eles moram em `%LOCALAPPDATA%\com.synqo.falcao-router`, que o
//! desinstalador preserva de propósito para quem reinstala — e o `shell.cmd` tem
//! a guarda `if not exist` justamente para ficar inofensivo sem o `router.exe`.
//! O `.bash_profile` também fica: é o mesmo arquivo que o Git for Windows criaria
//! sozinho, e apagá-lo devolveria o WARNING vermelho ao usuário.

use std::io;
use std::path::Path;

use router_core::engine::router_paths::RouterPaths;
use router_core::engine::shell_integration::{ShellIntegration, ShellTargets, SHIM_MARKER};
use router_core::platform::command_processor::{self, CommandProcessorKey};
use router_core::platform::profile_append::{remove_block, RemoveOutcome};

use crate::shared;

/// A chave do `AutoRun`: a de verdade, ou a de teste que `ROUTER_CMD_TEST_KEY`
/// nomeia. Sem esta saída a suíte apagaria o `AutoRun` de quem roda os testes —
/// um valor GLOBAL do usuário, compartilhado com clink, ConEmu e Anaconda.
fn autorun_key() -> CommandProcessorKey {
    match std::env::var("ROUTER_CMD_TEST_KEY") {
        Ok(name) if !name.is_empty() => CommandProcessorKey::for_tests(&name),
        _ => CommandProcessorKey::current_user(),
    }
}

/// Os dois `RemoveOutcome` (registro e perfil) dizem a mesma coisa; o do
/// registro vira o do perfil para a saída ser uma só.
fn autorun_removal() -> io::Result<RemoveOutcome> {
    command_processor::remove(&autorun_key(), SHIM_MARKER).map(|outcome| match outcome {
        command_processor::RemoveOutcome::Removed => RemoveOutcome::Removed,
        command_processor::RemoveOutcome::NotPresent => RemoveOutcome::NotPresent,
    })
}

/// Uma linha no estilo do `doctor`: `ok` tirou, `--` não havia o que tirar, `!!`
/// ficou para trás (e só este derruba o código de saída).
fn step(ok: &mut bool, what: &str, outcome: io::Result<RemoveOutcome>) {
    match outcome {
        Ok(RemoveOutcome::Removed) => println!("  ok  {what}: removido"),
        Ok(RemoveOutcome::NotPresent) => println!("  --  {what}: nada a remover"),
        Err(e) => {
            *ok = false;
            println!("  !!  {what}: {e}");
        }
    }
}

pub fn run() -> bool {
    println!("router uninstall-integration");
    let paths = RouterPaths::new();
    let targets = ShellTargets::for_user(Path::new(&shared::home()));
    let mut ok = true;

    // O `AutoRun` primeiro: é o que o gancho do desinstalador vem buscar, e o
    // pedaço que mais incomoda sobrando — ele roda antes de todo prompt do cmd.
    step(&mut ok, "AutoRun do Prompt de Comando", autorun_removal());

    // O marcador é a própria linha que a instalação escreveria HOJE — o mesmo
    // texto que o quadro usa para dizer "este perfil carrega a integração".
    let ps_line = ShellIntegration::powershell_source_line(&paths.base.join("shell.ps1"));
    for profile in &targets.powershell_profiles {
        let what = profile.display().to_string();
        step(&mut ok, &what, remove_block(profile, &ps_line));
    }
    let sh_line = ShellIntegration::bash_source_line(&paths.base.join("shell.sh"));
    let what = targets.bashrc.display().to_string();
    step(&mut ok, &what, remove_block(&targets.bashrc, &sh_line));

    println!(
        "{}",
        if ok {
            "\nintegração removida."
        } else {
            "\nficou o que está marcado com !! acima."
        }
    );
    ok
}
